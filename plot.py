import os
from pathlib import Path
from typing import List

import matplotlib
import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

matplotlib.rcParams.update({"font.size": 14})


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
    hue_order = [
        "causal",
        "optimistic-linear",
        "resettable-session",
        "monotonic-session",
        "synchronous",
    ]
    assert len(hue_order) == data["consistency"].nunique()
    hue = "consistency"

    # plt.figure()
    # ax = sns.scatterplot(
    #     data,
    #     x="duration_ms",
    #     y="total_states",
    #     hue=hue,
    #     hue_order=hue_order,
    # )
    # ax.set(ylabel="Total states")
    # plt.tight_layout()
    # plt.savefig(plots / "scatter-duration-states-consistency-all.png")
    # plt.close()

    # plt.figure()
    # datamax = data.groupby(["function", "consistency"]).max("total_states")
    # ax = sns.ecdfplot(
    #     datamax,
    #     x="total_states",
    #     hue=hue,
    #     hue_order=hue_order,
    # )
    # ax.set(xlabel="Total states")
    # plt.tight_layout()
    # plt.savefig(plots / "ecdf-states-consistency-all.svg")
    # plt.close()

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
    sns.move_legend(ax, "center right", bbox_to_anchor=(0.99, 0.7))
    ax.set(xlabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-states-consistency-controllers-maxdepth-all.svg")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency", "controllers", "max_depth"]).max(
        "total_states"
    )
    datamax["state_rate"] = datamax["total_states"] / 60
    ax = sns.catplot(
        kind="strip",
        data=datamax,
        x="consistency",
        order=hue_order,
        y="state_rate",
        hue=hue,
        hue_order=hue_order,
        legend=False,
        col="controllers",
        row="max_depth",
        linewidth=1,
        alpha=0.7,
        sharex=True,
        sharey=True,
    )
    ax.set(yscale="log")
    ax.set(xlabel="Consistency model", ylabel="States explored per second")
    ax.tick_params(axis="x", labelrotation=30)
    plt.tight_layout()
    plt.savefig(plots / "strip-states-consistency-controllers-maxdepth-all.svg")
    plt.close()

    # for max_depth in data["max_depth"].unique():
    #     for controllers in data["controllers"].unique():
    #         plt.figure()
    #         filterdata = data[data["max_depth"] == max_depth]
    #         filterdata = filterdata[filterdata["controllers"] == controllers]
    #         datamax = data.groupby(["function", "consistency"]).max(
    #             "total_states"
    #         )
    #         ax = sns.ecdfplot(
    #             filterdata,
    #             x="total_states",
    #             hue=hue,
    #             hue_order=hue_order,
    #         )
    #         # sns.move_legend(ax, "center right", bbox_to_anchor=(0.99, 0.7))
    #         ax.set(xlabel="Total states")
    #         plt.tight_layout()
    #         plt.savefig(plots / f"ecdf-states-consistency-controllers-{controllers}-maxdepth-{max_depth}.svg")
    #         plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency"]).max("total_states")
    ax = sns.boxplot(
        datamax, x="consistency", y="total_states", hue="consistency", legend=False
    )
    ax.set(ylabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "box-consistency-states-all.svg")
    plt.close()

    plt.figure()
    datamax = data.groupby(["function", "consistency"]).max("total_states")
    ax = sns.stripplot(
        datamax,
        x="consistency",
        y="total_states",
        hue="consistency",
        legend=False,
        alpha=0.7,
        linewidth=1,
    )
    ax.set(ylabel="Total states")
    plt.tight_layout()
    plt.savefig(plots / "strip-consistency-states-all.svg")
    plt.close()


def plot_depths(files: List[Path], plots: Path):
    data = pd.concat([pd.read_csv(p) for p in files])
    hue_order = [
        "causal",
        "optimistic-linear",
        "resettable-session",
        "monotonic-session",
        "synchronous",
    ]
    assert len(hue_order) == data["consistency"].nunique()
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

    for controllers in data["controllers"].unique():
        for max_depth in data["max_depth"].unique():
            filterdata = data[data["controllers"] == controllers]
            filterdata = filterdata[filterdata["max_depth"] == max_depth]
            plt.figure()
            ax = sns.ecdfplot(
                filterdata,
                x="depth",
                weights="count",
                hue=hue,
                hue_order=hue_order,
            )
            ax.set(xlabel="Depth")
            plt.tight_layout()
            plt.savefig(
                plots
                / f"ecdf-depth-count-consistency-controllers-{controllers}-maxdepth-{max_depth}.svg"
            )
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
    sns.move_legend(ax, "center right", bbox_to_anchor=(0.99, 0.7))
    ax.set(xlabel="Depth")
    plt.tight_layout()
    plt.savefig(plots / "ecdf-depth-count-consistency-controllers-maxdepth-all.svg")
    plt.close()


def state_stats(files: List[Path]):
    data = pd.concat([pd.read_csv(p) for p in files])

    grouped = data.groupby(["consistency", "max_depth", "controllers"]).max(
        "total_states"
    )
    print(grouped)
    print(grouped["duration_ms"] / grouped["total_states"])


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

        # for path in run_data_paths(out):
        #     print(path)
        #     plot_per_run(Path(path), plots)
        #
        # for path in run_depth_paths(out):
        #     print(path)
        #     plot_depths_per_run(Path(path), plots)

        print("Plotting all states")
        plot_states(run_data_paths(out), plots)
        print("Plotting all depths")
        plot_depths(run_depth_paths(out), plots)

        # state_stats(run_data_paths(out))


main()
