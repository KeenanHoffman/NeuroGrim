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

    /// Produce full agent-mode JSON output, or a prose orientation for agents.
    ///
    /// Default output is the canonical AgentOutput JSON envelope (machine-
    /// readable, used by A2A peers and ecosystem aggregation). With
    /// `--prose`, render an agent-friendly orientation summary covering
    /// Brain identity, current state, strongest signals, calls to action,
    /// available skills/hats, and federation peers — the "what is this
    /// Brain and what can I do here" answer for AI agents on entry.
    #[command(visible_alias = "divine")]
    Agent {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        #[arg(long)]
        hat: Option<String>,
        #[arg(long)]
        human_persona: Option<String>,
        /// Render a prose orientation summary instead of JSON (v3.2 A.1).
        #[arg(long)]
        prose: bool,
        /// Suppress ANSI colors. Honored only with `--prose`; the JSON
        /// path is plain-text by construction.
        #[arg(long)]
        plain: bool,
        /// v3.3 F4: list every declared domain in the prose signals
        /// section instead of capping at the top 3. Default behavior
        /// auto-expands when the Brain is all-advisory (no weighted
        /// domains), so this flag is mostly useful for weighted Brains
        /// where the operator wants the full picture.
        #[arg(long)]
        all_domains: bool,
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

    /// Narrate the Brain's state through a hat's lens (templated, no LLM)
    ///
    /// Produces 3-5 lines of hat-calibrated prose summarizing the
    /// Brain's score + trajectory + top concern + correlation count.
    /// Each declared hat (adversary, architect, incident-commander,
    /// rubber-duck, security-auditor, supply-chain-auditor,
    /// visionary) carries its own template; the per-hat communication
    /// contract in `.claude/skills/hats/SKILL.md` documents each
    /// hat's distillation style. Templates are deterministic data
    /// files (no LLM); v3.1 charter §3 locked decision 1.
    Narrate {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        /// Hat to narrate through. Required. Supported: adversary,
        /// architect, incident-commander, rubber-duck, security-auditor,
        /// supply-chain-auditor, visionary.
        #[arg(long)]
        hat: String,
    },

    /// Validate the brain-registry.json configuration
    #[command(visible_alias = "seal")]
    Validate {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
    },

    /// Validate Brain configuration end-to-end without scoring (v3.2 A.2).
    ///
    /// Distinct from `validate` (registry-shape only) and `health`/`score`
    /// (run the scoring pipeline). `doctor` reads the registry plus
    /// on-disk artifacts (CMDBs, culture.yaml, federation declarations)
    /// and reports configuration issues an agent should fix before
    /// relying on this Brain. Exit code 0 = clean, 1 = warnings, 2 =
    /// errors. Read-only — no ledger writes, no scoring.
    Doctor {
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        /// Suppress ANSI colors. Auto-detected on non-TTY stdout via the
        /// `colored` crate, but `--plain` makes the choice explicit.
        #[arg(long)]
        plain: bool,
    },

    /// Print bundled methodology primer for the named topic (v3.2 B).
    ///
    /// Eight self-contained topic files ship inside the binary
    /// (methodology, domain, sensor, hat, scoring, federation, cli,
    /// culture). Run `neurogrim explain` (no topic) to list topics
    /// with one-line summaries. Each topic stands alone — read in any
    /// order. Source of truth: `crates/neurogrim-cli/data/explain/`.
    Explain(commands::explain::Args),

    /// Launch the dashboard server (HTTP + embedded React UI).
    ///
    /// Opens an interactive browser-based view of this Brain — score
    /// gauge, domain table, trajectory charts, federation graph,
    /// skills index. Read-only by default; mutation endpoints
    /// (service start/stop, sensor refresh) are gated behind
    /// `--allow-mutations` (v3.5+).
    ///
    /// The dashboard binds to 127.0.0.1 only by default. The URL is
    /// always printed to stderr; pass `--no-browser` to skip the
    /// auto-launch.
    Ui {
        /// Path to the registry to inspect.
        #[arg(short, long, default_value = ".claude/brain-registry.json")]
        registry: String,
        /// Port to bind on. When omitted, the dashboard reads the
        /// project's `.claude/brain/ports.json` (auto-allocated from
        /// the IANA dynamic range 49152-65535 on first run). Pass an
        /// explicit value (e.g. `--port 8420`) to override; this does
        /// NOT update `ports.json`, so v3.4-era bookmarks keep working
        /// without disturbing the persisted allocation.
        #[arg(long)]
        port: Option<u16>,
        /// Bind address. Default 127.0.0.1 (loopback only). Setting this
        /// to 0.0.0.0 exposes the dashboard on the network — only do this
        /// when you have separate network-layer access controls.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// Don't auto-open the browser; print the URL and wait.
        #[arg(long)]
        no_browser: bool,
        /// v3.5.0 — enable mutation endpoints (service start/stop,
        /// sensor refresh). Off by default; the dashboard remains
        /// read-only unless this flag is passed. Mutations are
        /// surfaced in the UI as Start/Stop buttons in the
        /// Federation page; without `--allow-mutations` the
        /// frontend hides them entirely.
        #[arg(long)]
        allow_mutations: bool,
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
        /// Tool name: git-health, rust-health, code-quality, test-health, deploy-readiness, security-standards, coherence, human-comms, secret-refs, docker-topology, agent-behavior, skill-coherence, capability-hygiene, supply-chain-sca, supply-chain-vigilance, supply-chain-review, domain-calibration, operator-calibration, trust-budget, federated-patterns
        name: String,
        /// Project root path
        #[arg(long, default_value = ".")]
        project_root: String,
    },

    /// Initialize a new brain-registry.json by scanning the project
    ///
    /// v3.1.1+: When `--template <name>` is passed, additionally scaffolds
    /// the full Brain integration (culture.yaml, stub CMDBs, bundled
    /// skills, PostToolUse hook, CLAUDE.md, .gitignore extension).
    /// Without `--template`, behaves as before — registry generation only.
    ///
    /// Templates: `abstract-project` (no primary code; e.g., job-hunt),
    /// `code-project` (software project; default detection), `mixed`
    /// (both surfaces).
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
        /// Template to use for full scaffolding. Triggers Brain
        /// integration scaffolding beyond the registry. Supported:
        /// abstract-project, code-project, mixed. When omitted, only
        /// the registry is generated (legacy behavior).
        #[arg(long, value_parser = ["abstract-project", "code-project", "mixed"])]
        template: Option<String>,
        /// Project name override. Defaults to the directory name.
        /// Used in CLAUDE.md template substitution and registry meta.
        #[arg(long)]
        name: Option<String>,
        /// Comma-separated additional domain names to declare with
        /// stub CMDBs (advisory weight 0.0 each). Layered on top of
        /// the template's default domain set.
        #[arg(long)]
        domains: Option<String>,
        /// v3.3 F8: project-specific Brain description for `meta.description`.
        /// Defaults to a generic "initialized via `neurogrim init...`" string
        /// if omitted. Useful when the operator has bespoke framing for the
        /// Brain that won't fit a template.
        #[arg(long)]
        description: Option<String>,
        /// v3.3 F10: per-domain authoring intent, recorded as `_todo_<name>`
        /// on the domain's definition. Repeatable. Format: `NAME=DESCRIPTION`.
        /// Example: `--domain-describe "test-coverage=Sensor (when authored)
        /// will report uncovered modules from cargo-tarpaulin output."`
        /// Captures sensor intent so a future author has a starting point.
        #[arg(long)]
        domain_describe: Vec<String>,
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

    /// Operator-disposition CLI for the invocation ledger (LSP-Brains v2.11
    /// §17.12, E-B2-6). Single sub-command at v1: `record` appends a
    /// `DispositionEntry` row to
    /// `<project_root>/.claude/brain/invocation-ledger.jsonl` recording the
    /// operator's judgment of a prior skill invocation.
    ///
    /// `--kind` is validated at parse time against the closed-set 4-entry
    /// vocabulary (`accepted | rejected | modified | superseded`, Q1 lock).
    /// `--operator` falls back to `$NEUROGRIM_OPERATOR` (§17.6); both unset
    /// → error. NO `--note` flag at v1 (Q5 privacy lock — spec §17.12.3).
    /// Invocation IDs starting with `operator_calibration:` are rejected
    /// (Q6 recursion-guard MUST — spec §17.12.5).
    #[command(name = "disposition")]
    Disposition(commands::disposition::Args),

    /// Operator-explicit federated-pattern emission CLI (LSP-Brains v2.12
    /// §16.6.1, E-B2-7 C7). Single sub-command at v1: `emit` constructs a
    /// synthetic v1 `FederatedPatternPayload`, writes an
    /// `entry_kind=emitted` row to
    /// `<project_root>/.claude/brain/pattern-aggregation-ledger.jsonl`
    /// BEFORE transmission (Q12 log-before-transmit lock), and calls the
    /// protocol-layer `emit_federated_pattern` for each declared peer.
    ///
    /// `--pattern-kind` is validated at parse time against the closed-set
    /// v1 single-entry vocabulary `["vigilance-pattern"]` (Q14 lock).
    /// Pattern-kind values starting with `federated_patterns:` are
    /// rejected (Q9 recursion-guard MUST — spec §16.6.1). NO free-text
    /// flags at v1 (Q1+Q5+Q8 privacy lock — spec §16.6.1).
    #[command(name = "federated-pattern")]
    FederatedPattern(commands::federated_pattern::Args),

    /// Serve this Brain as an A2A peer (spec §13). Publishes an Agent Card
    /// and accepts peer invocations (snapshot.requested, score.updated ack).
    #[command(name = "a2a-serve", visible_alias = "beacon")]
    A2aServe {
        /// TCP port to bind. When omitted, the server reads
        /// `<project_root>/.claude/brain/ports.json` (auto-allocated
        /// from the IANA dynamic range 49152-65535 on first run).
        /// Pass an explicit value (e.g. `--port 8421`) to override
        /// without disturbing the persisted allocation.
        #[arg(long)]
        port: Option<u16>,
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

    /// Skill workflow commands. v3.1.1+: `neurogrim skill new <name>`
    /// scaffolds a SKILL.md skeleton for a project-specific skill.
    Skill(commands::skill::Args),

    /// v4.0 (S12-G-2) — quiet test wrapper with persisted failure
    /// ledger. Runs `cargo test --workspace --all-targets`,
    /// suppresses success spam, prints failures inline, appends one
    /// JSONL entry per failure to
    /// `<project_root>/.claude/brain/test-failures.jsonl`. Mirrors
    /// cargo's exit code (0 on all-pass, 1 on any-fail). Flags:
    /// `--keep-last N` (rotate older entries to archive),
    /// `--show-only-new` (diff against prior run), `--retry-failed`
    /// (replay only the most recent failure batch), `--slow` (include
    /// `#[ignore]`d benchmarks), `--verbose` (bypass parser; stream
    /// cargo's output).
    Test(commands::test::Args),

    /// v4.0 (S12-G-4) — publish-gate runner. Two sub-commands:
    /// `run` executes the manifest's gates and emits a per-gate JSONL
    /// entry to `<brain>/.claude/brain/publish-gate-ledger.jsonl`;
    /// `ack` marks the most recent pending entry for `--gate <id>` as
    /// passed (operator handle from `--operator` or
    /// `$NEUROGRIM_OPERATOR`). `run` exit code: 0 all blocking gates
    /// passed, 1 any blocking failed/timed_out, 2 any blocking
    /// pending operator. `--mode` heuristic (v1): pre-commit = fast
    /// automated only, pre-publish = blocking only, full = every
    /// gate. e2e gates ship as `deferred` until S12-G-5 wires the
    /// Playwright harness.
    #[command(name = "publish-gate")]
    PublishGate(commands::publish_gate::Args),

    /// v4.1 (S13-B-7) — agent coordination bus CLI. Sub-commands:
    /// `list` (every topic on disk + stats), `tail <topic>` (last N
    /// messages), `publish <topic> <json>` (manual produce; agents
    /// use the MCP `queue_publish` tool), `stats <topic>` (single-
    /// topic stats as JSON). Reads/writes
    /// `<project>/.claude/brain/queues/<topic>.jsonl`. v1 ships
    /// JSONL-backed only; `compact`, `migrate`, and `inspect`
    /// sub-commands land with the SQLite backend in S13-B-3.
    Queue(commands::queue::Args),

    /// Domain workflow commands. v3.2: `neurogrim domain new <name>`
    /// scaffolds a new domain (registry mutation + stub CMDB +
    /// optional Python sensor skeleton).
    Domain(commands::domain::Args),

    /// Federation workflow commands. v3.1.1+: `neurogrim federation register`
    /// adds a child Brain to the local registry (ecosystem-coordinator
    /// workflow; supports the `--read-only` flag for sibling-project peers).
    Federation(commands::federation::Args),

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

    /// v4.2 S14-S-4.5 — secrets-management lifecycle. v1 ships
    /// `tls-cert {generate, fingerprint, status, rotate}` for the
    /// dashboard's secret-management endpoints. Future stories add
    /// `secret {get, set, list, rotate}` once the CLI surface for
    /// the SecretBackend lands.
    Secrets(commands::secrets::Args),
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
            prose,
            plain,
            all_domains,
        } => commands::agent::run(&registry, hat, human_persona, prose, plain, all_domains).await,
        Commands::Health {
            registry,
            plain,
            hat,
            human_persona,
        } => commands::health::run(&registry, hat, human_persona, plain).await,
        Commands::Trend { registry, plain } => commands::trend::run(&registry, plain).await,
        Commands::Narrate { registry, hat } => commands::narrate::run(&registry, hat).await,
        Commands::Validate { registry } => commands::validate::run(&registry).await,
        Commands::Doctor { registry, plain } => commands::doctor::run(&registry, plain).await,
        Commands::Explain(args) => commands::explain::run(args).await,
        Commands::Ui {
            registry,
            port,
            bind,
            no_browser,
            allow_mutations,
        } => commands::ui::run(registry, port, bind, no_browser, allow_mutations).await,
        Commands::Serve { registry } => commands::serve::run(&registry).await,
        Commands::Sensory { name, project_root } => run_sensory(&name, &project_root).await,
        Commands::Init {
            project_root,
            output,
            yes,
            template,
            name,
            domains,
            description,
            domain_describe,
        } => commands::init::run(
            &project_root,
            &output,
            yes,
            template,
            name,
            domains,
            description,
            domain_describe,
        )
        .await,
        Commands::Awareness {
            project_root,
            subcommand,
        } => commands::awareness::run(&project_root, subcommand).await,
        Commands::ScaReview { subcommand } => commands::sca_review::run(subcommand).await,
        Commands::ScaCalibrate(args) => commands::sca_calibrate::run(args).await,
        Commands::DomainCalibration { subcommand } => {
            commands::domain_calibration::run(subcommand).await
        }
        Commands::Disposition(args) => commands::disposition::run(args).await,
        Commands::FederatedPattern(args) => commands::federated_pattern::run(args).await,
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
        Commands::Skill(args) => commands::skill::run(args).await,
        Commands::Test(args) => commands::test::run(args).await,
        Commands::PublishGate(args) => commands::publish_gate::run(args).await,
        Commands::Queue(args) => commands::queue::run(args).await,
        Commands::Domain(args) => commands::domain::run(args).await,
        Commands::Federation(args) => commands::federation::run(args).await,
        Commands::Secrets(args) => commands::secrets::run(args).await,
    }
}

async fn run_sensory(name: &str, project_root: &str) -> Result<()> {
    eprintln!("✦ Casting {name}…");
    let result = match name {
        "git-health" => neurogrim_sensory::git_health::analyze_git_health(project_root).await?,
        "rust-health" => neurogrim_sensory::rust_health::analyze_rust_health(project_root).await,
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
        "operator-calibration" => neurogrim_sensory::operator_calibration::analyze_operator_calibration(project_root).await,
        "trust-budget" => neurogrim_sensory::trust_budget::analyze_trust_budget(project_root).await,
        "federated-patterns" => neurogrim_sensory::federated_patterns::analyze_federated_patterns(project_root).await,
        _ => anyhow::bail!("Unknown sensory tool: {}. Available: git-health, rust-health, code-quality, test-health, deploy-readiness, security-standards, coherence, human-comms, secret-refs, docker-topology, agent-behavior, skill-coherence, capability-hygiene, supply-chain-sca, supply-chain-vigilance, supply-chain-review, domain-calibration, operator-calibration, trust-budget, federated-patterns", name),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
