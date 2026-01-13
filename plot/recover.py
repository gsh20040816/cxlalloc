import polars as pl
import plotly
import plotly.io as pio
import plotly.graph_objects as go
import plotly.subplots as sp
import sys
import common

# https://github.com/plotly/plotly.py/issues/3469
pio.kaleido.scope.mathjax = None

ALLOCATOR = "Allocator"
CRASH_COUNT = "Crash Count"
PHASE = "Phase"
PHASE_GC = "GC"
PHASE_EXEC = "Execution"
WORKLOAD = "Workload"
LEAK_SIZE = "Leak Size"
TIME = "Execution Time (s)"
TIME_TOTAL = "Total Time"

ORDER_ALLOCATOR = ["cxlalloc", "ralloc-leak", "ralloc-gc"]
ORDER_PHASE = [PHASE_GC, PHASE_EXEC]

PATTERN = {
    PHASE_GC: "x",
    PHASE_EXEC: "",
}
COLOR_ALLOCATOR = {
    "cxlalloc": "black",
    "ralloc-leak": common.SCHEME[1],
    "ralloc-gc": "lightgray",
}


def main():
    df = pl.scan_ndjson(sys.argv[1])

    df = (
        df.select(
            pl.when(pl.col("config").struct["allocator"] == "cxlalloc")
            .then(pl.lit("cxlalloc"))
            .when(
                (pl.col("config").struct["allocator"] == "ralloc")
                & pl.col("config").struct["block"]
            )
            .then(pl.lit("ralloc-gc"))
            .otherwise(pl.lit("ralloc-leak"))
            .alias(ALLOCATOR),
            pl.when(pl.col("config").struct["workload"] == "clevel")
            .then(pl.lit("hashmap"))
            .otherwise(pl.col("config").struct["workload"])
            .alias(WORKLOAD),
            pl.col("config").struct["crash_count"].alias(CRASH_COUNT),
            pl.col("output").struct["time"].alias(TIME_TOTAL),
            pl.col("output").struct["gc_time"].alias(PHASE_GC),
            pl.col("output").struct["cache_size"].alias(LEAK_SIZE),
        )
        .filter(pl.col(CRASH_COUNT).is_in([0, 1, 2]))
        .with_columns(
            pl.col(LEAK_SIZE) / 2**10,
            pl.col(TIME_TOTAL) / 1e6,
            pl.col(PHASE_GC) / 1e6,
        )
    )

    workloads = df.select(WORKLOAD).unique(maintain_order=True).collect().to_series()
    crash_counts = (
        df.select(CRASH_COUNT).unique(maintain_order=True).collect().to_series().sort()
    )

    fig = sp.make_subplots(
        rows=1,
        cols=2,
        # x_title="Crash Count (threads)",
        column_titles=workloads.to_list(),
    )

    for col, workload in enumerate(workloads):
        for offset, allocator in enumerate(ORDER_ALLOCATOR):
            data = df.filter(
                (pl.col(WORKLOAD) == workload) & (pl.col(ALLOCATOR) == allocator)
            ).collect()

            trace = go.Bar(
                name=allocator,
                x=data[CRASH_COUNT],
                y=data[TIME_TOTAL],
                legendgroup=allocator,
                marker_color=COLOR_ALLOCATOR[allocator],
            )

            fig.add_trace(trace, row=1, col=col + 1)

            for row, crash_count in enumerate(crash_counts):
                filtered = data.filter(pl.col(CRASH_COUNT) == crash_count)
                gc = filtered[PHASE_GC].item()
                total = filtered[TIME_TOTAL].item()
                leak = filtered[LEAK_SIZE].item()

                if gc > 0:
                    fig.add_shape(
                        name=PHASE_GC,
                        showlegend=False,
                        type="rect",
                        xref=f"x{col + 1}",
                        x0=row + 0.125,
                        x1=row + 0.375,
                        yref=f"y{col + 1}",
                        y0=0.0,
                        y1=gc,
                        line_color="black",
                        # fillcolor="red",
                        # opacity=0.2,
                        # line_width=0,
                        label=dict(
                            text=f"GC {gc / total * 100.0:.0f}%",
                            textangle=-90,
                            font_color="red",
                            font_size=16,
                        ),
                    )

                if leak > 0:
                    fig.add_annotation(
                        x=row,
                        y=total + 1,
                        showarrow=True,
                        arrowhead=3,
                        arrowsize=2,
                        xref=f"x{col + 1}",
                        text=f"Leak {leak:.1f} KiB",
                        font_size=16,
                        textangle=-90,
                        xanchor="left",
                        font_color="red",
                    )

    unique = set()
    # https://stackoverflow.com/a/62162555
    fig.for_each_trace(
        lambda trace: trace.update(showlegend=False)
        if (trace.name in unique)
        else unique.add(trace.name)
    )

    fig.for_each_xaxis(
        lambda xaxis: xaxis.update(
            title=dict(text="Crash Count (#threads)", font_size=common.SIZE_XAXIS_TITLE)
        ),
        row=1,
        col=1,
    )

    fig.update_yaxes(
        title=dict(text="Execution Time (secs)", font_size=common.SIZE_YAXIS_TITLE),
        col=1,
    )
    fig.update_layout(
        width=600,
        height=400,
        # title="Memento Partial Failure Workloads",
        template=common.THEME,
        margin=dict(l=0, r=0, t=20, b=50),
        legend=dict(
            # title=dict(text="Allocator", font_size=16),
            font_size=common.SIZE_LEGEND_ENTRY,
            orientation="h",
            xanchor="right",
            yanchor="top",
            y=-0.06,
            # y=-0.16,
            x=1.0,
        ),
    )
    fig.write_image("out.pdf")
    fig.show()


if __name__ == "__main__":
    main()
