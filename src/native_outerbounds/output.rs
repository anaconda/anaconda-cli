use comfy_table::{Cell, ContentArrangement, Table, presets::UTF8_FULL_CONDENSED};

pub fn create_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);

    let header_cells: Vec<Cell> = headers.iter().map(Cell::new).collect();
    table.set_header(header_cells);
    table
}

pub fn print_table(table: Table) {
    println!("{table}");
}
