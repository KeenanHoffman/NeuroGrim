use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod output;

#[derive(Parser)]
#[command(name = "motherbrain")]
#[command(about = "MotherBrain — LSP Brains scoring engine")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compute and display the unified health score
    Score {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        /// Output as plain text (no ANSI colors)
        #[arg(long)]
        plain: bool,
        /// Active hat for domain emphasis
        #[arg(long)]
        hat: Option<String>,
        /// Output persona (executive, manager, developer, specialist, product-manager)
        #[arg(long)]
        persona: Option<String>,
    },

    /// Produce full agent-mode JSON output
    Agent {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        #[arg(long)]
        hat: Option<String>,
        #[arg(long)]
        persona: Option<String>,
    },

    /// Display human-readable health dashboard
    Health {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        #[arg(long)]
        plain: bool,
        #[arg(long)]
        hat: Option<String>,
        #[arg(long)]
        persona: Option<String>,
    },

    /// Show trajectory analysis (velocity, acceleration, classification)
    Trend {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        #[arg(long)]
        plain: bool,
    },

    /// Validate the brain-registry.json configuration
    Validate {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
    },

    /// Start the Brain as an MCP server
    Serve {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
    },

    /// Run a built-in sensory tool directly (produces CMDB JSON)
    Sensory {
        /// Tool name: git-health, code-quality, test-health, deploy-readiness, security-standards, coherence, human-comms, secret-refs
        name: String,
        /// Project root path
        #[arg(long, default_value = ".")]
        project_root: String,
    },

    /// Initialize a new brain-registry.json by scanning the project
    Init {
        /// Project root to scan
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Output path for brain-registry.json
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        output: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Manage local machine-specific awareness (tool paths, OS quirks, known patterns)
    Awareness {
        /// Project root
        #[arg(long, default_value = ".")]
        project_root: String,
        #[command(subcommand)]
        subcommand: Option<commands::awareness::AwarenessCmd>,
    },

    /// Serve this Brain as an A2A peer (spec §13). Publishes an Agent Card
    /// and accepts peer invocations (snapshot.requested, score.updated ack).
    #[command(name = "a2a-serve")]
    A2aServe {
        /// TCP port to bind.
        #[arg(long, default_value_t = commands::a2a_serve::DEFAULT_PORT)]
        port: u16,
        /// Interface to bind on. Defaults to `127.0.0.1` (loopback-only —
        /// the safe default that matches the unit and integration tests).
        /// Container deployments should pass `--bind 0.0.0.0` so the port
        /// is reachable from outside the container's network namespace.
        /// Spec §13.6 mandates `authentication: none` in v2.1, so when you
        /// bind to a non-loopback address you MUST gate access at the
        /// network layer (Docker bridge, firewall, VPN, or cloud VPC).
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// Project root containing `.claude/brain-registry.json`.
        #[arg(long, default_value = ".")]
        project_root: String,
        /// Optional override path to a JSON-encoded AgentCard (advanced; v1
        /// is passthrough — the registry-derived card is still the default).
        #[arg(long)]
        agent_card: Option<String>,
    },

    /// Invoke a single A2A message against a peer Brain (spec §13.3).
    #[command(name = "a2a-invoke")]
    A2aInvoke {
        /// Peer base URL, e.g. `http://127.0.0.1:8421/a2a/v1/`.
        peer_url: String,
        /// Message type: snapshot.requested, score.updated, etc.
        #[arg(long, default_value = "snapshot.requested")]
        message_type: String,
        /// JSON payload. Defaults to `{}`.
        #[arg(long)]
        payload: Option<String>,
    },

    /// Fetch a peer Brain's Agent Card (spec §13.2). Prints the card.
    #[command(name = "a2a-discover")]
    A2aDiscover {
        /// Peer base URL, e.g. `http://127.0.0.1:8421/a2a/v1/`.
        peer_url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Score { registry, plain, hat, persona } => {
            commands::score::run(&registry, plain, hat, persona).await
        }
        Commands::Agent { registry, hat, persona } => {
            commands::agent::run(&registry, hat, persona).await
        }
        Commands::Health { registry, plain, hat, persona } => {
            commands::health::run(&registry, hat, persona, plain).await
        }
        Commands::Trend { registry, plain } => {
            commands::trend::run(&registry, plain).await
        }
        Commands::Validate { registry } => {
            commands::validate::run(&registry).await
        }
        Commands::Serve { registry } => {
            commands::serve::run(&registry).await
        }
        Commands::Sensory { name, project_root } => {
            run_sensory(&name, &project_root).await
        }
        Commands::Init { project_root, output, yes } => {
            commands::init::run(&project_root, &output, yes).await
        }
        Commands::Awareness { project_root, subcommand } => {
            commands::awareness::run(&project_root, subcommand).await
        }
        Commands::A2aServe { port, bind, project_root, agent_card } => {
            commands::a2a_serve::run(port, bind, project_root, agent_card).await
        }
        Commands::A2aInvoke { peer_url, message_type, payload } => {
            commands::a2a_invoke::run(peer_url, message_type, payload).await
        }
        Commands::A2aDiscover { peer_url } => {
            commands::a2a_discover::run(peer_url).await
        }
    }
}

async fn run_sensory(name: &str, project_root: &str) -> Result<()> {
    let result = match name {
        "git-health" => motherbrain_sensory::git_health::analyze_git_health(project_root).await?,
        "code-quality" => motherbrain_sensory::code_quality::analyze_code_quality(project_root).await,
        "test-health" => motherbrain_sensory::test_results::analyze_test_health(project_root).await,
        "deploy-readiness" => motherbrain_sensory::deploy_readiness::analyze_deploy_readiness(project_root).await,
        "security-standards" => motherbrain_sensory::security_standards::analyze_security_standards(project_root).await,
        "coherence" => motherbrain_sensory::coherence::analyze_coherence(project_root).await,
        "human-comms" => motherbrain_sensory::human_comms::analyze_human_comms(project_root).await,
        "secret-refs" => motherbrain_sensory::secret_refs::analyze_secret_refs(project_root).await,
        _ => anyhow::bail!("Unknown sensory tool: {}. Available: git-health, code-quality, test-health, deploy-readiness, security-standards, coherence, human-comms, secret-refs", name),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
