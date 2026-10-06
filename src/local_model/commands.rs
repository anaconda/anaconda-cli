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

/// Catalog size classes (published as `size-<class>` tags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ModelSize {
    Tiny,
    Small,
    Medium,
    Large,
    Xlarge,
}

impl ModelSize {
    pub fn name(&self) -> &'static str {
        match self {
            ModelSize::Tiny => "tiny",
            ModelSize::Small => "small",
            ModelSize::Medium => "medium",
            ModelSize::Large => "large",
            ModelSize::Xlarge => "xlarge",
        }
    }
}

#[derive(Subcommand)]
pub enum LocalModelCommands {
    /// List models in the catalog, or downloaded models with --local
    List {
        /// Only show models downloaded to ~/.ana/models
        #[arg(long)]
        local: bool,

        /// Filter by model name (substring, e.g., qwen)
        #[arg(short = 'n', long, visible_alias = "search", conflicts_with = "local")]
        name: Option<String>,

        /// Filter by publisher (e.g., Qwen, google, nvidia)
        #[arg(short = 'p', long, conflicts_with = "local")]
        publisher: Option<String>,

        /// Filter by purpose (e.g., text-generation, image-text-to-text, sentence-similarity)
        #[arg(long, conflicts_with = "local")]
        purpose: Option<String>,

        /// Filter by tag; repeat to require several (e.g., chat, tool-calling, reasoning)
        #[arg(short = 't', long = "tag", conflicts_with = "local")]
        tags: Vec<String>,

        /// Filter by size class; repeat to allow several [possible values: tiny, small, medium, large, xlarge]
        #[arg(long = "size", value_enum, conflicts_with = "local")]
        sizes: Vec<ModelSize>,

        /// Filter by file format or quantization (e.g., gguf, safetensors, q4_k_m)
        #[arg(short = 'f', long, conflicts_with = "local")]
        file: Option<String>,

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
