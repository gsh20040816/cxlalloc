import plotly.graph_objects as go
import common
import sys
import polars as pl


def main():
    df = pl.scan_ndjson(sys.argv[1], infer_schema_length=None)

    df = common.collapse(
        df,
        workloads=common.MICRO_WORKLOADS,
    )

    metrics = [common.THROUGHPUT]
    fig = common.make_subplots(common.MICRO_WORKLOADS, metrics=metrics)

    for col, workload in enumerate(common.MICRO_WORKLOADS):
        for row, metric in enumerate(metrics):
            for allocator in common.ALLOCATORS:
                if allocator == common.Allocator.MIMALLOC:
                    continue

                data = (
                    df.filter(pl.col(common.ALLOCATOR) == allocator)
                    .filter(pl.col(common.WORKLOAD) == workload)
                    .collect()
                )

                trace = common.style(
                    allocator,
                    go.Scatter,
                    error_y=dict(array=data[metric + "_std"]),
                    x=data[common.THREAD_COUNT],
                    y=data[metric],
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    fig.for_each_yaxis(lambda yaxis: yaxis.update(type="log"), row=1)

    common.update_layout(fig, full=False, numa=False, single_row=True)
    fig.write_image("out.pdf")
    fig.show()


if __name__ == "__main__":
    main()
