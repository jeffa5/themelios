use stateright::Checker;
use stateright::HasDiscoveries;
use stateright::Model;
use stateright::UniformChooser;
use std::path::Path;
use std::time::Duration;
use themelios::model::OrchestrationModelCfg;
use themelios::report::JointReporter;
use themelios::report::StdoutReporter;
use themelios::report::CSVReporter;
use tracing::info;

macro_rules! test_table {
    { $globalname:ident, $name:ident($consistency:expr, $controllers:expr) } => {
        paste::item! {
            #[test_log::test]
            fn [< $globalname _ $name >]() {
                $globalname($consistency, $controllers)
            }
        }
    };
    { $global_name:ident, $name:ident($consistency:expr, $controllers:expr), $($x:ident($y:expr, $z:expr)),+ } => {
        test_table! { $global_name, $name($consistency, $controllers) }
        test_table! { $global_name, $($x($y, $z)),+ }
    }
}

pub(crate) use test_table;

pub fn run(model: OrchestrationModelCfg, fn_name: &str) {
    println!("Running test {:?}", fn_name);

    if let Ok(explore_test) = std::env::var("MCO_EXPLORE_TEST") {
        if fn_name.ends_with(&explore_test) {
            let path = std::env::var("MCO_EXPLORE_PATH").unwrap_or_default();
            explore(model, path);
        }
        // skip others
    } else {
        check(model, fn_name)
    }
}

fn check(model: OrchestrationModelCfg, test_name:&str) {
    println!("Checking model");
    let am = model.into_abstract_model();
    let mut reporter = JointReporter {
        reporters: vec![
            Box::new(StdoutReporter::new(&am)),
            Box::new(CSVReporter::new(Path::new(test_name))),
        ],
    };
    let checker = am
        .checker()
        .threads(num_cpus::get())
        .finish_when(HasDiscoveries::AnyFailures)
        .target_max_depth(100)
        .timeout(Duration::from_secs(60));
    let check_mode = std::env::var("MCO_CHECK_MODE").unwrap_or_else(|_| String::new());
    #[allow(clippy::wildcard_in_or_patterns)]
    let check_result = match check_mode.as_str() {
        "dfs" => {
            info!(check_mode, "Running checking");
            checker.spawn_dfs().report(&mut reporter).check_properties()
        }
        "bfs" => {
            info!(check_mode, "Running checking");
            checker.spawn_bfs().report(&mut reporter).check_properties()
        }
        "simulation" | _ => {
            info!(check_mode, "Running checking");
            checker
                .spawn_simulation(0, UniformChooser)
                .report(&mut reporter)
                .check_properties()
        }
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
