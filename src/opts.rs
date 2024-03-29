use clap::Parser;

#[derive(Parser, Debug)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: SubCmd,

    /// The number of threads to run.
    /// Defaults to the number of CPUs the machine has, as reported by `num_cpus`.
    #[clap(long, short, global = true)]
    pub threads: Option<usize>,

    #[clap(long, short, global = true, default_value = "1")]
    pub initial_pods: u32,

    #[clap(long, global = true, default_value = "1")]
    pub replicasets: u32,

    #[clap(long, global = true, default_value = "1")]
    pub pods_per_replicaset: u32,

    #[clap(long, global = true, default_value = "1")]
    pub replicaset_controllers: usize,

    #[clap(long, global = true, default_value = "1")]
    pub deployments: u32,

    #[clap(long, global = true, default_value = "1")]
    pub deployment_controllers: usize,

    #[clap(long, global = true, default_value = "1")]
    pub statefulsets: u32,

    #[clap(long, global = true, default_value = "1")]
    pub pods_per_statefulset: u32,

    #[clap(long, global = true, default_value = "1")]
    pub statefulset_controllers: usize,

    #[clap(long, global = true, default_value = "1")]
    pub job_controllers: usize,

    #[clap(long, global = true, default_value = "1")]
    pub podgc_controllers: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub schedulers: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub nodes: usize,

    /// Max depth for the check run, 0 is no limit.
    #[clap(long, global = true, default_value = "0")]
    pub max_depth: usize,

    /// Model session consistency for the state.
    #[clap(long, global = true)]
    pub session: bool,

    /// Model optimistic linear consistency for the state.
    #[clap(long, global = true)]
    pub optimistic_linear: bool,

    /// Model causal consistency for the state.
    #[clap(long, global = true)]
    pub causal: bool,
}

#[derive(clap::Subcommand, Debug)]
pub enum SubCmd {
    Explore {
        /// Path to a state.
        fingerprint_path: Option<String>,
        /// Port to serve the UI on.
        #[clap(long, default_value = "8080")]
        port: u16,
    },
    CheckDfs,
    CheckBfs,
    CheckSimulation {
        #[clap(long)]
        seed: Option<u64>,
    },
    /// Serve an integration test suitable API.
    ServeTest {
        #[clap(long, default_value = "7070")]
        port: u16,
    },
    /// Deploy as API server, interactable with kubectl.
    ServeCluster {
        #[clap(long, default_value = "8080")]
        port: u16,
    },
    /// Deploy as controller-manager.
    ControllerManager {},
}
