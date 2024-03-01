import os
from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

out = Path("testout")
plots = Path("plots")


def plot_per_run(d: Path, path: Path):
    data = pd.read_csv(d / path)
    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="total_states")
    plt.tight_layout()
    plt.savefig(plots / f"states-{path}.png")
    plt.close()

    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="max_depth")
    plt.tight_layout()
    plt.savefig(plots / f"depth-{path}.png")
    plt.close()


def plot_states(d: Path):
    data = pd.concat([pd.read_csv(d/p) for p in os.listdir(d)])
    plt.figure()
    ax = sns.lineplot(data, x="duration_ms", y="total_states", hue="consistency")
    plt.tight_layout()
    plt.savefig(plots / "all-states.png")
    plt.close()

for path in os.listdir(out):
    print(path)
    plot_per_run(out, Path(path))

plot_states(out)
