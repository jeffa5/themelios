import os
from pathlib import Path
from typing import List

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns


def plot_per_run(path: Path, plots: Path):
    data = pd.read_csv(path)
    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="total_states")
    ax.set(ylabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / f"line-duration-states-{path.stem}.svg")
    plt.close()

    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="max_depth_reached")
    ax.set(ylabel="Max depth reached")
    plt.tight_layout()
    plt.savefig(plots / f"line-duration-maxdepth-{path.stem}.svg")
    plt.close()


def plot_depths_per_run(path: Path, plots: Path):
    data = pd.read_csv(path)
    plt.figure()
    ax = sns.scatterplot(data, x="depth", y="count")
    ax.set(xlabel="Depth")
    plt.tight_layout()
    plt.savefig(plots / f"scatter-depth-count-{path.stem}.png")
    plt.close()


def plot_states(files: List[Path], plots: Path):
    data = pd.concat([pd.read_csv(p) for p in files])
    hue_order = sorted(data["consistency"].unique())
    hue = "consistency"

    plt.figure()
    ax = sns.scatterplot(
        data,
        x="duration_ms",
        y="total_states",
        hue=hue,
        hue_order=hue_order,
    )
    ax.set(ylabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "scatter-duration-states-consistency-all.png")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency"]).max("total_states")
    ax = sns.ecdfplot(
        datamax,
        x="total_states",
        hue=hue,
        hue_order=hue_order,
    )
    ax.set(xlabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-states-consistency-all.svg")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency", "controllers", "max_depth"]).max(
        "total_states"
    )
    ax = sns.displot(
        kind="ecdf",
        data=datamax,
        x="total_states",
        hue=hue,
        hue_order=hue_order,
        col="controllers",
        row="max_depth",
    )
    sns.move_legend(ax, "center right", bbox_to_anchor=(0.99, 0.3))
    ax.set(xlabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-states-consistency-controllers-maxdepth-all.svg")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency"]).max("total_states")
    ax = sns.boxplot(datamax, x="consistency", y="total_states")
    ax.set(ylabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "box-consistency-states-all.svg")
    plt.close()


def plot_depths(files: List[Path], plots: Path):
    data = pd.concat([pd.read_csv(p) for p in files])
    hue_order = sorted(data["consistency"].unique())
    hue = "consistency"

    plt.figure()
    ax = sns.scatterplot(
        data,
        x="depth",
        y="count",
        hue=hue,
        hue_order=hue_order,
    )
    ax.set(xlabel="Depth", ylabel="Count")
    plt.tight_layout()
    plt.savefig(plots / "scatter-depth-count-consistency-all.png")
    plt.close()

    plt.figure()
    ax = sns.ecdfplot(
        data,
        x="depth",
        weights="count",
        hue=hue,
        hue_order=hue_order,
    )
    ax.set(xlabel="Depth")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-depth-count-consistency-all.svg")
    plt.close()

    plt.figure()
    ax = sns.displot(
        kind="ecdf",
        data=data,
        x="depth",
        weights="count",
        hue=hue,
        hue_order=hue_order,
        col="controllers",
        row="max_depth",
    )
    sns.move_legend(ax, "center right", bbox_to_anchor=(0.99, 0.3))
    ax.set(xlabel="Depth")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-depth-count-consistency-controllers-maxdepth-all.svg")
    plt.close()


def run_data_paths(d: Path) -> List[Path]:
    return [d / Path(p) for p in os.listdir(d) if "-depths" not in p]


def run_depth_paths(d: Path) -> List[Path]:
    return [d / Path(p) for p in os.listdir(d) if "-depths" in p]


def main():
    for out, plots in [
        (Path("testout"), Path("plots")),
        (Path("coverageout"), Path("covplots")),
    ]:
        plots.mkdir(exist_ok=True)

        for path in run_data_paths(out):
            print(path)
            plot_per_run(Path(path), plots)

        for path in run_depth_paths(out):
            print(path)
            plot_depths_per_run(Path(path), plots)

        print("Plotting all states")
        plot_states(run_data_paths(out), plots)
        print("Plotting all depths")
        plot_depths(run_depth_paths(out), plots)


main()
