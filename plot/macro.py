import common

from common import (
    ALLOCATOR,
    ALLOCATORS,
    THREAD_COUNT,
    WORKLOAD,
    METRICS,
)

import polars as pl
import plotly.graph_objects as go


def main():
    df = common.scan_ndjson()
    df = common.collapse(
        df,
        workloads=common.MACRO_WORKLOADS,
    )

    thread_counts = df.select(THREAD_COUNT).unique().collect().to_series().sort()

    fig = common.make_subplots(common.MACRO_WORKLOADS)

    for col, workload in enumerate(common.MACRO_WORKLOADS):
        for row, metric in enumerate(METRICS):
            for allocator in ALLOCATORS:
                data = (
                    df.filter(pl.col(ALLOCATOR) == allocator)
                    .filter(pl.col(WORKLOAD) == workload)
                    .collect()
                )

                trace = common.style(
                    allocator,
                    go.Scatter,
                    x=data[THREAD_COUNT],
                    y=data[metric],
                    error_y=dict(array=data[metric + "_std"]),
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    fig.for_each_yaxis(lambda yaxis: yaxis.update(type="log"), row=1)

    # Clip lightning RSS
    for col, workload in enumerate(common.MACRO_WORKLOADS):
        data = (
            df.filter(pl.col(WORKLOAD) == workload)
            .select(common.PSS)
            .sort(common.PSS)
            .collect()
            .head(-len(thread_counts))
            .to_series()
        )

        # low = data.first() * 0.99
        low = 0.0
        high = data.last() * 1.1

        fig.for_each_yaxis(
            lambda yaxis: yaxis.update(range=(low, high)),
            col=col + 1,
            row=2,
        )

    common.update_layout(fig, full=True, numa=True)

    fig.write_image("macro.pdf")
    fig.show()


if __name__ == "__main__":
    main()
