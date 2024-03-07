import os
from pathlib import Path
from typing import List

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

out = Path("testout")
plots = Path("plots")


def plot_per_run(path: Path):
    data = pd.read_csv(path)
    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="total_states")
    plt.tight_layout()
    plt.savefig(plots / f"line-duration-states-{path.stem}.png")
    plt.close()

    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="max_depth")
    plt.tight_layout()
    plt.savefig(plots / f"line-duration-maxdepth-{path.stem}.png")
    plt.close()


def plot_depths_per_run(path: Path):
    data = pd.read_csv(path)
    plt.figure()
    ax = sns.scatterplot(data, x="depth", y="count")
    plt.tight_layout()
    plt.savefig(plots / f"scatter-depth-count-{path.stem}.png")
    plt.close()


def plot_states(files: List[Path]):
    data = pd.concat([pd.read_csv(p) for p in files])
    plt.figure()
    ax = sns.scatterplot(data, x="duration_ms", y="total_states", hue="consistency")
    plt.tight_layout()
    plt.savefig(plots / "scatter-duration-states-consistency-all.png")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency"]).max("total_states")
    ax = sns.ecdfplot(datamax, x="total_states", hue="consistency")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-states-consistency-all.png")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency", "controllers"]).max(
        "total_states"
    )
    ax = sns.displot(
        kind="ecdf",
        data=datamax,
        x="total_states",
        hue="consistency",
        col="controllers",
    )
    plt.tight_layout()
    plt.savefig(plots / "ecdf-states-consistency-controllers-all.png")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency"]).max("total_states")
    ax = sns.boxplot(datamax, x="consistency", y="total_states")
    plt.tight_layout()
    plt.savefig(plots / "box-consistency-states-all.png")
    plt.close()


def plot_depths(files: List[Path]):
    data = pd.concat([pd.read_csv(p) for p in files])

    plt.figure()
    ax = sns.scatterplot(data, x="depth", y="count", hue="consistency")
    plt.tight_layout()
    plt.savefig(plots / "scatter-depth-count-consistency-all.png")
    plt.close()

    plt.figure()
    ax = sns.ecdfplot(data, x="depth", weights="count", hue="consistency")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-depth-count-consistency-all.png")
    plt.close()

    plt.figure()
    ax = sns.displot(
        kind="ecdf",
        data=data,
        x="depth",
        weights="count",
        hue="consistency",
        col="controllers",
    )
    plt.tight_layout()
    plt.savefig(plots / "ecdf-depth-count-consistency-controllers-all.png")
    plt.close()


def run_data_paths(d: Path) -> List[Path]:
    return [d / Path(p) for p in os.listdir(d) if "-depths" not in p]


def run_depth_paths(d: Path) -> List[Path]:
    return [d / Path(p) for p in os.listdir(d) if "-depths" in p]


def main():
    plots.mkdir(exist_ok=True)

    for path in run_data_paths(out):
        print(path)
        plot_per_run(Path(path))

    for path in run_depth_paths(out):
        print(path)
        plot_depths_per_run(Path(path))

    print("Plotting all states")
    plot_states(run_data_paths(out))
    print("Plotting all depths")
    plot_depths(run_depth_paths(out))


main()
