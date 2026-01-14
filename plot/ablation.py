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
    pl.Config.set_tbl_rows(-1)

    print(
        df.collect().select(
            pl.col(common.WORKLOAD).filter(
                pl.col(common.ALLOCATOR) == common.Allocator.CXLALLOC_MCAS
            ),
            pl.col(common.THREAD_COUNT).filter(
                pl.col(common.ALLOCATOR) == common.Allocator.CXLALLOC_MCAS
            ),
            (
                pl.col(common.THROUGHPUT).filter(
                    pl.col(common.ALLOCATOR) == common.Allocator.CXLALLOC_MCAS
                )
                / pl.col(common.THROUGHPUT).filter(
                    pl.col(common.ALLOCATOR) == common.Allocator.CXLALLOC_HWCC
                )
            ).alias("Throughput relative to hwcc"),
            (
                pl.col(common.THROUGHPUT).filter(
                    pl.col(common.ALLOCATOR) == common.Allocator.CXLALLOC_MCAS
                )
                / pl.col(common.THROUGHPUT).filter(
                    pl.col(common.ALLOCATOR) == common.Allocator.RALLOC_MCAS
                )
            ).alias("Throughput relative to ralloc-mcas"),
        )
    )

    metrics = [common.THROUGHPUT]
    fig = common.make_subplots(common.MICRO_WORKLOADS, metrics=metrics)

    for col, workload in enumerate(common.MICRO_WORKLOADS):
        for row, metric in enumerate(metrics):
            for allocator in [
                common.Allocator.CXLALLOC,
                common.Allocator.CXLALLOC_HWCC,
                common.Allocator.CXLALLOC_MCAS,
                common.Allocator.RALLOC,
                common.Allocator.RALLOC_HWCC,
                common.Allocator.RALLOC_MCAS,
            ]:
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
                    legend={
                        common.Allocator.CXLALLOC: "legend1",
                        common.Allocator.RALLOC: "legend2",
                    }[allocator.split("-")[0]],
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    fig.for_each_yaxis(lambda yaxis: yaxis.update(type="log"), row=1)

    common.update_layout(fig, full=False, numa=False, single_row=True)
    fig.update_layout(
        height=225,
        legend2=dict(
            title=dict(text="", font_size=common.SIZE_LEGEND_TITLE),
            orientation="h",
            xanchor="left",
            yanchor="top",
            font_size=common.SIZE_LEGEND_ENTRY,
            y=-0.4,
            x=-0.05,
            tracegroupgap=0,
        ),
    )

    fig.write_image("ablation.pdf")
    fig.show()


if __name__ == "__main__":
    main()
