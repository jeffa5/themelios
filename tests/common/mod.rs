use stateright::Checker;
use stateright::HasDiscoveries;
use stateright::Model;
use stateright::UniformChooser;
use std::time::Duration;
use themelios::model::OrchestrationModelCfg;
use themelios::report::Reporter;
use themelios::state::history::ConsistencySetup;
use tracing::info;

pub fn run(mut model: OrchestrationModelCfg, default_check_mode: CheckMode, fn_name: &str) {
    println!("Running test {:?}", fn_name);

    if let Ok(consistency_level) = std::env::var("MCO_CONSISTENCY") {
        let consistency_level = match consistency_level.as_str() {
            "linearizable" => ConsistencySetup::Linearizable,
            "monotonic-session" => ConsistencySetup::MonotonicSession,
            "resettable-session" => ConsistencySetup::ResettableSession,
            _ => {
                panic!(
                    "Unknown consistency level from MCO_CONSISTENCY: {:?}",
                    consistency_level
                )
            }
        };
        info!(?consistency_level, "Set consistency level from environment");
        model.consistency_level = consistency_level;
    }

    if let Ok(explore_test) = std::env::var("MCO_EXPLORE_TEST") {
        if fn_name.ends_with(&explore_test) {
            let path = std::env::var("MCO_EXPLORE_PATH").unwrap_or_default();
            explore(model, path);
            return;
        } else {
            // skip others
            return;
        }
    }

    check(model, default_check_mode)
}

#[allow(dead_code)]
pub enum CheckMode {
    Bfs,
    Dfs,
    Simulation(Duration),
}

fn check(model: OrchestrationModelCfg, default_check_mode: CheckMode) {
    println!("Checking model");
    let am = model.into_abstract_model();
    let mut reporter = Reporter::new(&am);
    let checker = am
        .checker()
        .threads(num_cpus::get())
        .finish_when(HasDiscoveries::AnyFailures)
        .target_max_depth(100)
        .timeout(Duration::from_secs(60));
    let check_mode = std::env::var("MCO_CHECK_MODE").unwrap_or_else(|_| String::new());
    let check_result = match check_mode.as_str() {
        "simulation" => {
            info!(check_mode, "Running checking");
            checker
                .spawn_simulation(0, UniformChooser)
                .report(&mut reporter)
                .check_properties()
        }
        "dfs" => {
            info!(check_mode, "Running checking");
            checker.spawn_dfs().report(&mut reporter).check_properties()
        }
        "bfs" => {
            info!(check_mode, "Running checking");
            checker.spawn_bfs().report(&mut reporter).check_properties()
        }
        _ => match default_check_mode {
            CheckMode::Bfs => checker.spawn_bfs().report(&mut reporter).check_properties(),
            CheckMode::Dfs => checker.spawn_dfs().report(&mut reporter).check_properties(),
            CheckMode::Simulation(timeout) => checker
                .timeout(timeout)
                .spawn_simulation(0, UniformChooser)
                .report(&mut reporter)
                .check_properties(),
        },
    };
    if !check_result.iter().all(|(_, ok)| *ok) {
        panic!("Some properties failed");
    }
}

fn explore(model: OrchestrationModelCfg, mut path: String) {
    let host = "127.0.0.1";
    let port = 8080;
    if !path.is_empty() {
        path = format!("#/steps/{path}");
    }
    println!(
        "Exploring model, served on http://{}:{}/{}",
        host, port, path
    );
    let am = model.into_abstract_model();
    am.checker().serve((host, port));
}
