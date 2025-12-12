import common
from common import (
    PROCESS_COUNT,
    THREAD_COUNT,
    WORKLOAD,
)
import polars as pl
import plotly.graph_objects as go


def main():
    df = common.scan_ndjson()
    df = common.collapse(
        df,
        workloads=common.HUGE_WORKLOADS,
    )

    fig = common.make_subplots(common.HUGE_WORKLOADS)
    # process_counts = (
    #     df.select(common.PROCESS_COUNT).unique().collect().to_series().sort()
    # )

    process_counts = [1, 2, 10, 40, 80]

    for col, workload in enumerate(common.HUGE_WORKLOADS):
        for row, metric in enumerate(common.METRICS):
            for process_count in process_counts:
                data = (
                    df.filter(pl.col(PROCESS_COUNT) == process_count)
                    .filter(pl.col(WORKLOAD) == workload)
                    .collect()
                )

                trace = go.Scatter(
                    x=data[THREAD_COUNT],
                    y=data[metric],
                    error_y=dict(array=data[metric + "_std"]),
                    line=dict(color="black"),
                    marker=dict(
                        symbol={
                            1: "line-ns",
                            2: "y-up",
                            5: "x-thin",
                            10: "circle",
                            20: "triangle-up",
                            40: "square",
                            80: "pentagon",
                        }[process_count],
                        size=12,
                        line_width=4 if process_count < 10 else 0,
                    ),
                    name=process_count,
                    legendgroup=process_count,
                    zorder=-process_count,
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    common.update_layout(fig, full=False, numa=True)
    fig.update_layout(legend_title="Process Count")
    fig.write_image("huge.pdf")
    fig.show()


if __name__ == "__main__":
    main()
