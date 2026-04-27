use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod output;

#[derive(Parser)]
#[command(name = "neurogrim")]
#[command(about = "NeuroGrim — LSP Brains scoring engine")]
#[command(long_about = "NeuroGrim — LSP Brains scoring engine\n\na book of spells for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compute and display the unified health score
    #[command(visible_alias = "scry")]
    Score {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        /// Output as plain text (no ANSI colors)
        #[arg(long)]
        plain: bool,
        /// Active hat for domain emphasis
        #[arg(long)]
        hat: Option<String>,
        /// Output human-persona (executive, manager, developer, specialist, product-manager)
        #[arg(long)]
        human_persona: Option<String>,
    },

    /// Produce full agent-mode JSON output
    #[command(visible_alias = "divine")]
    Agent {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        #[arg(long)]
        hat: Option<String>,
        #[arg(long)]
        human_persona: Option<String>,
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
        human_persona: Option<String>,
    },

    /// Show trajectory analysis (velocity, acceleration, classification)
    #[command(visible_alias = "drift")]
    Trend {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        #[arg(long)]
        plain: bool,
    },

    /// Validate the brain-registry.json configuration
    #[command(visible_alias = "seal")]
    Validate {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
    },

    /// Start the Brain as an MCP server
    #[command(visible_alias = "summon")]
    Serve {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
    },

    /// Run a built-in sensory tool directly (produces CMDB JSON)
    #[command(visible_alias = "cast")]
    Sensory {
        /// Tool name: git-health, code-quality, test-health, deploy-readiness, security-standards, coherence, human-comms, secret-refs, docker-topology, agent-behavior, skill-coherence, capability-hygiene, supply-chain-sca, supply-chain-vigilance, supply-chain-review, domain-calibration, trust-budget
        name: String,
        /// Project root path
        #[arg(long, default_value = ".")]
        project_root: String,
    },

    /// Initialize a new brain-registry.json by scanning the project
    #[command(visible_alias = "conjure")]
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

    /// Supply-chain Layer 3 review CLI — open / list / resolve review tickets
    /// (LSP-Brains v2.6 §16.4). Tickets carry the human-decision gate for
    /// flagged dependencies; resolution writes to the append-only decision
    /// ledger at .claude/supply-chain-decision-ledger.jsonl.
    #[command(name = "sca-review")]
    ScaReview {
        #[command(subcommand)]
        subcommand: commands::sca_review::ScaReviewCmd,
    },

    /// Run supply-chain calibration against the fixture library and emit a
    /// calibration report (E-SC-8). Use --check-promotion-ready to gate CI on
    /// promotion-readiness; in v1 this always returns exit 1 by design (the
    /// framework ships before the data does).
    #[command(name = "sca-calibrate")]
    ScaCalibrate(commands::sca_calibrate::ScaCalibrateArgs),

    /// Per-domain calibration ledger triage CLI (LSP-Brains v2.8 §17, E-B2-2).
    /// Three actions: `list` (inspect open + triaged entries), `triage`
    /// (record a 4-class decision against an open pending entry), `manual`
    /// (operator-initiated pending entry — the default writer for domains
    /// configured with `calibration_trigger: Manual`).
    ///
    /// All write paths require operator identity via `--operator <handle>`
    /// or `$NEUROGRIM_OPERATOR` (§17.6). All write paths validate the
    /// `--domain` arg against `brain-registry.json`'s `domain_weights`
    /// (§17.2 — registry is the authoritative domain enum).
    #[command(name = "domain-calibration")]
    DomainCalibration {
        #[command(subcommand)]
        subcommand: commands::domain_calibration::DomainCalibrationCmd,
    },

    /// Serve this Brain as an A2A peer (spec §13). Publishes an Agent Card
    /// and accepts peer invocations (snapshot.requested, score.updated ack).
    #[command(name = "a2a-serve", visible_alias = "beacon")]
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
        /// Require `Authorization: Bearer <token>` on task requests. When
        /// set, the Agent Card advertises `authentication.scheme: bearer`
        /// and all `/a2a/v1/tasks*` requests must present a valid token.
        /// `/.well-known/agent-card.json` stays public regardless.
        #[arg(long)]
        require_bearer: bool,
        /// Path to the token-store sqlite file. Defaults to
        /// `<project-root>/.claude/a2a-tokens.sqlite`. Only consulted when
        /// `--require-bearer` is set.
        #[arg(long)]
        token_store: Option<String>,
    },

    /// Invoke a single A2A message against a peer Brain (spec §13.3).
    #[command(name = "a2a-invoke", visible_alias = "commune")]
    A2aInvoke {
        /// Peer base URL, e.g. `http://127.0.0.1:8421/a2a/v1/`.
        peer_url: String,
        /// Message type: snapshot.requested, score.updated, etc.
        #[arg(long, default_value = "snapshot.requested")]
        message_type: String,
        /// JSON payload. Defaults to `{}`.
        #[arg(long)]
        payload: Option<String>,
        /// Bearer token for peers that require bearer auth. If omitted and
        /// the environment variable `NEUROGRIM_A2A_TOKEN` is set, that value
        /// is used instead. Required for peers whose Agent Card declares
        /// `authentication.scheme: bearer`; harmless for `scheme: none`.
        #[arg(long)]
        bearer: Option<String>,
    },

    /// Fetch a peer Brain's Agent Card (spec §13.2). Prints the card.
    #[command(name = "a2a-discover", visible_alias = "behold")]
    A2aDiscover {
        /// Peer base URL, e.g. `http://127.0.0.1:8421/a2a/v1/`.
        peer_url: String,
    },

    /// Manage A2A bearer tokens: issue, list, revoke (spec §13 + bearer).
    ///
    /// Tokens are stored in a sqlite database under the project root; only
    /// the hash of each token is persisted. See `neurogrim a2a-token --help`.
    #[command(name = "a2a-token")]
    A2aToken {
        /// Path to the token store sqlite file. Defaults to
        /// `.claude/a2a-tokens.sqlite` under the current directory.
        #[arg(long, default_value = ".claude/a2a-tokens.sqlite")]
        store: String,
        #[command(subcommand)]
        subcommand: commands::a2a_token::A2aTokenCmd,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Score {
            registry,
            plain,
            hat,
            human_persona,
        } => commands::score::run(&registry, plain, hat, human_persona).await,
        Commands::Agent {
            registry,
            hat,
            human_persona,
        } => commands::agent::run(&registry, hat, human_persona).await,
        Commands::Health {
            registry,
            plain,
            hat,
            human_persona,
        } => commands::health::run(&registry, hat, human_persona, plain).await,
        Commands::Trend { registry, plain } => commands::trend::run(&registry, plain).await,
        Commands::Validate { registry } => commands::validate::run(&registry).await,
        Commands::Serve { registry } => commands::serve::run(&registry).await,
        Commands::Sensory { name, project_root } => run_sensory(&name, &project_root).await,
        Commands::Init {
            project_root,
            output,
            yes,
        } => commands::init::run(&project_root, &output, yes).await,
        Commands::Awareness {
            project_root,
            subcommand,
        } => commands::awareness::run(&project_root, subcommand).await,
        Commands::ScaReview { subcommand } => commands::sca_review::run(subcommand).await,
        Commands::ScaCalibrate(args) => commands::sca_calibrate::run(args).await,
        Commands::DomainCalibration { subcommand } => {
            commands::domain_calibration::run(subcommand).await
        }
        Commands::A2aServe {
            port,
            bind,
            project_root,
            agent_card,
            require_bearer,
            token_store,
        } => {
            commands::a2a_serve::run(
                port,
                bind,
                project_root,
                agent_card,
                require_bearer,
                token_store,
            )
            .await
        }
        Commands::A2aInvoke {
            peer_url,
            message_type,
            payload,
            bearer,
        } => {
            let bearer = bearer.or_else(|| std::env::var("NEUROGRIM_A2A_TOKEN").ok());
            commands::a2a_invoke::run(peer_url, message_type, payload, bearer).await
        }
        Commands::A2aDiscover { peer_url } => commands::a2a_discover::run(peer_url).await,
        Commands::A2aToken { store, subcommand } => {
            commands::a2a_token::run(store, subcommand).await
        }
    }
}

async fn run_sensory(name: &str, project_root: &str) -> Result<()> {
    eprintln!("✦ Casting {name}…");
    let result = match name {
        "git-health" => neurogrim_sensory::git_health::analyze_git_health(project_root).await?,
        "code-quality" => neurogrim_sensory::code_quality::analyze_code_quality(project_root).await,
        "test-health" => neurogrim_sensory::test_results::analyze_test_health(project_root).await,
        "deploy-readiness" => neurogrim_sensory::deploy_readiness::analyze_deploy_readiness(project_root).await,
        "security-standards" => neurogrim_sensory::security_standards::analyze_security_standards(project_root).await,
        "coherence" => neurogrim_sensory::coherence::analyze_coherence(project_root).await,
        "human-comms" => neurogrim_sensory::human_comms::analyze_human_comms(project_root).await,
        "secret-refs" => neurogrim_sensory::secret_refs::analyze_secret_refs(project_root).await,
        "docker-topology" => neurogrim_sensory::docker_topology::analyze_docker_topology(project_root).await?,
        "agent-behavior" => neurogrim_sensory::agent_behavior::analyze_agent_behavior(project_root).await?,
        "skill-coherence" => neurogrim_sensory::skill_coherence::analyze_skill_coherence(project_root).await,
        "capability-hygiene" => neurogrim_sensory::capability_hygiene::analyze_capability_hygiene(project_root).await,
        "supply-chain-sca" => neurogrim_sensory::supply_chain_sca::analyze_supply_chain_sca(project_root).await,
        "supply-chain-vigilance" => neurogrim_sensory::supply_chain_vigilance::analyze_supply_chain_vigilance(project_root).await,
        "supply-chain-review" => neurogrim_sensory::supply_chain_review::analyze_supply_chain_review(project_root).await,
        "domain-calibration" => neurogrim_sensory::domain_calibration::analyze_domain_calibration(project_root).await,
        "trust-budget" => neurogrim_sensory::trust_budget::analyze_trust_budget(project_root).await,
        _ => anyhow::bail!("Unknown sensory tool: {}. Available: git-health, code-quality, test-health, deploy-readiness, security-standards, coherence, human-comms, secret-refs, docker-topology, agent-behavior, skill-coherence, capability-hygiene, supply-chain-sca, supply-chain-vigilance, supply-chain-review, domain-calibration, trust-budget", name),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
