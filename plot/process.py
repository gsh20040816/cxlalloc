import common
from common import (
    ALLOCATOR,
    THROUGHPUT,
    PROCESS_COUNT,
    THREAD_COUNT,
    WORKLOAD,
)
import polars as pl
import plotly.graph_objects as go
import plotly.subplots as sp

MACRO = True

WORKLOADS = common.MACRO_WORKLOADS if MACRO else common.MICRO_WORKLOADS


def main():
    df = common.scan_ndjson()
    df = common.collapse(
        df,
        workloads=WORKLOADS,
    )

    thread_counts = df.select(THREAD_COUNT).unique().collect().to_series().sort()

    fig = sp.make_subplots(
        rows=len(thread_counts),
        cols=len(WORKLOADS),
        shared_xaxes=True,
        column_titles=WORKLOADS,
        horizontal_spacing=0.03,
        vertical_spacing=0.10,
    )

    for col, workload in enumerate(WORKLOADS):
        for row, thread_count in enumerate(thread_counts):
            for allocator in common.ALLOCATORS:
                data = (
                    df.filter(pl.col(THREAD_COUNT) == thread_count)
                    .filter(pl.col(ALLOCATOR) == allocator)
                    .filter(pl.col(WORKLOAD) == workload)
                    .collect()
                )

                trace = common.style(
                    allocator,
                    go.Scatter,
                    x=data[PROCESS_COUNT],
                    y=data[THROUGHPUT],
                    error_y=dict(array=data[THROUGHPUT + "_std"]),
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    common.update_layout(fig, full=MACRO, numa=False)

    fig.for_each_xaxis(
        lambda xaxis: xaxis.update(title_text="Process Count"),
        row=2,
        col=1 if MACRO else None,
    )

    for row, thread_count in enumerate(thread_counts):
        fig.for_each_yaxis(
            lambda yaxis: yaxis.update(title_text=f"Throughput@{thread_count}T"),
            row=row + 1,
            col=1,
        )

    fig.for_each_yaxis(lambda yaxis: yaxis.update(range=[0, None], rangemode="tozero"))
    fig.show()
    fig.write_image("out.pdf")


if __name__ == "__main__":
    main()
