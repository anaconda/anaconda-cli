use clap::{Subcommand, ValueEnum};
use std::fmt;

/// Model formats supported by the model catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModelFormat {
    #[value(name = "safetensor")]
    Safetensor,
    #[value(name = "gguf")]
    Gguf,
}

impl ModelFormat {
    pub fn name(&self) -> &'static str {
        match self {
            ModelFormat::Safetensor => "safetensor",
            ModelFormat::Gguf => "gguf",
        }
    }
}

impl fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Subcommand)]
pub enum LocalModelCommands {
    /// List local models
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Download a model from the model catalog
    Pull {
        /// Catalog model name (e.g., Qwen/Qwen2.5-7B-Instruct)
        #[arg(required_unless_present = "help", default_value = "")]
        model: String,

        /// Model format to download [possible values: safetensor, gguf]
        #[arg(long, value_enum, required_unless_present = "help")]
        format: Option<ModelFormat>,

        /// Exact GGUF filename to download
        #[arg(long)]
        file: Option<String>,

        /// GGUF quantization (e.g., q4_k_m, q8_0)
        #[arg(long)]
        quant: Option<String>,
    },

    /// Serve a model in the background with llama-server, downloading it first if needed
    Run {
        /// Model name (e.g., Qwen/Qwen2.5-0.5B-Instruct) or path to a .gguf file
        #[arg(required_unless_present = "help", default_value = "")]
        model: String,

        /// GGUF quantization to use (e.g., q4_k_m, q8_0). Defaults to q4_k_m when downloading
        #[arg(long)]
        quant: Option<String>,

        /// Host address to bind the server to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// Additional arguments passed to llama-server (after `--`)
        #[arg(last = true)]
        llama_args: Vec<String>,
    },

    /// Delete a downloaded model, or a single quantization of it
    Delete {
        /// Model name (e.g., Qwen/Qwen2.5-0.5B-Instruct)
        #[arg(required_unless_present = "help", default_value = "")]
        model: String,

        /// Only delete this GGUF quantization (e.g., q8_0). Deletes the whole model if omitted
        #[arg(long)]
        quant: Option<String>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        force: bool,
    },
}
