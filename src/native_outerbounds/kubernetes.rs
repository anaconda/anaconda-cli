use miette::{Result, miette};
use outerbounds::KillOptions;

use crate::context::CommandContext;
use crate::input::prompt_yes_no;
use crate::ui::status;

use super::output::{create_table, print_table};

#[allow(clippy::too_many_arguments)]
pub async fn kill(
    ctx: &CommandContext,
    flow_name: &str,
    run_id: Option<&str>,
    my_runs: bool,
    dry_run: bool,
    auto_approve: bool,
    clear_everything: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let user = if my_runs {
        let config = ob
            .config()
            .ok_or_else(|| miette!("Config not loaded. Run 'ana obn configure' first."))?;
        let user = config
            .user
            .clone()
            .ok_or_else(|| miette!("METAFLOW_USER not found in config"))?;
        status::info(&format!("Filtering for runs by user: {}", user));
        Some(user)
    } else {
        None
    };

    status::info(&format!(
        "Searching for jobs and jobsets matching flow: {}",
        flow_name
    ));
    if let Some(rid) = run_id {
        status::info(&format!("Filtering by run ID: {}", rid));
    }

    let (jobs, jobsets) = ob
        .kubernetes()
        .list_matching(flow_name, run_id, user.as_deref())
        .await?;

    let total_found = jobs.len() + jobsets.len();
    if total_found == 0 {
        status::success("No matching jobs or jobsets found.");
        return Ok(());
    }

    let jobs_to_delete = jobs
        .iter()
        .filter(|j| clear_everything || j.outcome == outerbounds::JobOutcome::Delete)
        .count();
    let jobsets_to_delete = jobsets
        .iter()
        .filter(|j| clear_everything || j.outcome == outerbounds::JobOutcome::Delete)
        .count();

    if clear_everything {
        status::warn(
            "CLEAR EVERYTHING MODE: All matching resources will be force deleted regardless of status!",
        );
    }

    if !jobs.is_empty() {
        println!("\nJobs:");
        let mut table = create_table(&["Name", "Run ID", "User", "Status", "Action"]);
        for job in &jobs {
            let will_delete = clear_everything || job.outcome == outerbounds::JobOutcome::Delete;
            let action = if will_delete { "delete" } else { "keep" };
            table.add_row(vec![
                &job.name,
                job.run_id.as_deref().unwrap_or("-"),
                job.user.as_deref().unwrap_or("-"),
                &job.status_description,
                action,
            ]);
        }
        print_table(table);
    }

    if !jobsets.is_empty() {
        println!("\nJobSets:");
        let mut table = create_table(&["Name", "Run ID", "User", "Status", "Action"]);
        for jobset in &jobsets {
            let will_delete = clear_everything || jobset.outcome == outerbounds::JobOutcome::Delete;
            let action = if will_delete { "delete" } else { "keep" };
            table.add_row(vec![
                &jobset.name,
                jobset.run_id.as_deref().unwrap_or("-"),
                jobset.user.as_deref().unwrap_or("-"),
                &jobset.status_description,
                action,
            ]);
        }
        print_table(table);
    }

    if dry_run {
        status::info(&format!(
            "Dry run: would delete {} job(s) and {} jobset(s)",
            jobs_to_delete, jobsets_to_delete
        ));
        return Ok(());
    }

    if jobs_to_delete + jobsets_to_delete == 0 {
        status::success("Nothing to delete.");
        return Ok(());
    }

    if !auto_approve
        && !prompt_yes_no(
            &format!(
                "Delete {} job(s) and {} jobset(s)?",
                jobs_to_delete, jobsets_to_delete
            ),
            false,
        )
    {
        status::info("Aborted.");
        return Ok(());
    }

    let result = ob
        .kubernetes()
        .kill(KillOptions {
            flow_name: flow_name.to_string(),
            run_id: run_id.map(String::from),
            user,
            dry_run,
            clear_everything,
        })
        .await?;

    status::success(&format!(
        "Deleted {} job(s) and {} jobset(s)",
        result.jobs_deleted, result.jobsets_deleted
    ));

    for err in &result.errors {
        status::warn(err);
    }

    Ok(())
}
