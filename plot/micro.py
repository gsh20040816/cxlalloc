import common
from common import ALLOCATOR, ALLOCATORS, THREAD_COUNT, WORKLOAD, PSS
import polars as pl
import plotly.graph_objects as go
import plotly.subplots as sp
import sys


def main():
    df = common.scan_ndjson()
    df = common.collapse(
        df,
        workloads=common.MICRO_WORKLOADS,
    ).filter(
        # Skip benchmarks that took too long to run
        (
            pl.col(ALLOCATOR).is_in(
                [common.Allocator.BOOST, common.Allocator.LIGHTNING]
            )
            & (pl.col(WORKLOAD) == common.Workload.THREADTEST_LARGE)
            & (pl.col(THREAD_COUNT) > 40)
        ).not_()
    )

    thread_counts = df.select(THREAD_COUNT).unique().collect().to_series().sort()

    fig = common.make_subplots(common.MICRO_WORKLOADS)

    for col, workload in enumerate(common.MICRO_WORKLOADS):
        for row, metric in enumerate(common.METRICS):
            for allocator in ALLOCATORS:
                data = (
                    df.filter(pl.col(ALLOCATOR) == allocator)
                    .filter(pl.col(WORKLOAD) == workload)
                    .collect()
                )

                trace = common.style(
                    allocator,
                    go.Scatter,
                    error_y=dict(array=data[metric + "_std"]),
                    x=data[THREAD_COUNT],
                    y=data[metric],
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    fig.for_each_yaxis(lambda yaxis: yaxis.update(type="log"), row=1)

    # Clip lightning RSS
    for col, workload in enumerate(common.MICRO_WORKLOADS):
        data = (
            df.filter(pl.col(WORKLOAD) == workload)
            .select(PSS)
            .sort(PSS)
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

    common.update_layout(fig, full=False, numa=True)

    fig.write_image("micro.pdf")
    fig.show()


if __name__ == "__main__":
    main()
