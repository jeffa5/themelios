use model_checked_orchestration::model::OrchestrationModelCfg;
use model_checked_orchestration::report::Reporter;
use stateright::Checker;
use stateright::HasDiscoveries;
use stateright::Model;
use stateright::UniformChooser;
use std::collections::BTreeMap;
use std::time::Duration;

use model_checked_orchestration::resources::Meta;

// Check that the annotations on resource `a` are all set on resource `b`.
#[allow(dead_code)]
pub fn annotations_subset<T, U>(a: &T, b: &U) -> bool
where
    T: Meta,
    U: Meta,
{
    subset(&a.metadata().annotations, &b.metadata().annotations)
}

#[allow(dead_code)]
fn subset(m1: &BTreeMap<String, String>, m2: &BTreeMap<String, String>) -> bool {
    m1.iter().all(|(k, v)| m2.get(k).map_or(false, |w| v == w))
}

pub fn run(model: OrchestrationModelCfg, default_check_mode: CheckMode, fn_name: &str) {
    println!("Running test {:?}", fn_name);
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
        .target_max_depth(30);
    let check_mode = std::env::var("MCO_CHECK_MODE").unwrap_or_else(|_| String::new());
    // skip clippy bit here for clarity
    #[allow(clippy::wildcard_in_or_patterns)]
    match check_mode.as_str() {
        "simulation" => checker
            .spawn_simulation(0, UniformChooser)
            .report(&mut reporter)
            .assert_properties(),
        "dfs" => checker
            .spawn_dfs()
            .report(&mut reporter)
            .assert_properties(),
        "bfs" => checker
            .spawn_bfs()
            .report(&mut reporter)
            .assert_properties(),
        _ => match default_check_mode {
            CheckMode::Bfs => checker
                .spawn_bfs()
                .report(&mut reporter)
                .assert_properties(),
            CheckMode::Dfs => checker
                .spawn_dfs()
                .report(&mut reporter)
                .assert_properties(),
            CheckMode::Simulation(timeout) => checker
                .timeout(timeout)
                .spawn_simulation(0, UniformChooser)
                .report(&mut reporter)
                .assert_properties(),
        },
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
