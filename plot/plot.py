import polars as pl
import plotly.io as pio
import plotly.graph_objects as go
import plotly.express as px
import plotly.subplots as sp
import sys

# https://github.com/plotly/plotly.py/issues/3469
pio.kaleido.scope.mathjax = None

ALLOCATOR = "Allocator"
THREAD_COUNT = "Thread Count"
WORKLOAD = "workload"
THROUGHPUT = "Throughput (ops/sec)"
MAX_RSS = "Max RSS (GiB)"
SCHEME = px.colors.qualitative.D3
THEME = "plotly_white"

COLORS = {
    "cxlalloc": "black",
    "mimalloc": SCHEME[0],
    "ralloc": SCHEME[1],
    "cxl_shm": SCHEME[2],
    "boost": SCHEME[3],
    "lightning": SCHEME[4],
}


def main():
    df = pl.scan_ndjson(sys.argv[1], infer_schema_length=None)

    df = (
        df.group_by("date")
        .agg(
            pl.col("allocator").struct["name"].alias(ALLOCATOR),
            pl.col("global").struct["thread_count"].alias(THREAD_COUNT),
            pl.when(pl.col("benchmark").struct["trace"].str.contains("12"))
            .then(pl.lit("MC-12"))
            .when(pl.col("benchmark").struct["trace"].str.contains("15"))
            .then(pl.lit("MC-15"))
            .when(pl.col("benchmark").struct["trace"].str.contains("31"))
            .then(pl.lit("MC-31"))
            .when(pl.col("benchmark").struct["trace"].str.contains("37"))
            .then(pl.lit("MC-37"))
            .when(pl.col("benchmark").struct["insert_proportion"] > 0.9)
            .then(pl.lit("YCSB-Load"))
            .when(pl.col("benchmark").struct["insert_proportion"] < 0.06)
            .then(pl.lit("YCSB-D"))
            .otherwise(pl.lit("YCSB-A"))
            .alias(WORKLOAD),
            (
                pl.col("output")
                .struct["thread"]
                .list.explode()
                .struct["operation_count"]
                / pl.col("output").struct["thread"].list.explode().struct["time"]
                * 1e9
            )
            .sum()
            .alias(THROUGHPUT),
            pl.col("output")
            .struct["process"]
            .struct["resource_usage"]
            .struct["max_rss"]
            .sum()
            .truediv(2**30)
            .alias(MAX_RSS),
        )
        .explode(ALLOCATOR, THREAD_COUNT, WORKLOAD)
        # cxl-shm doesn't support allocations >= 1KiB
        .filter(
            (
                (
                    pl.col(WORKLOAD).str.contains("12")
                    | pl.col(WORKLOAD).str.contains("37")
                )
                & (pl.col(ALLOCATOR) == "cxl_shm")
            ).not_()
        )
        .sort(ALLOCATOR, WORKLOAD, THREAD_COUNT)
    )

    workloads = ["YCSB-Load", "YCSB-A", "YCSB-D", "MC-12", "MC-15", "MC-31", "MC-37"]
    metrics = [THROUGHPUT, MAX_RSS]
    allocators = ["cxlalloc", "mimalloc", "ralloc", "cxl_shm", "boost", "lightning"]
    thread_counts = df.select(THREAD_COUNT).unique().collect().to_series().sort()

    fig = sp.make_subplots(
        rows=len(metrics),
        cols=len(workloads),
        shared_xaxes=True,
        column_titles=workloads,
        vertical_spacing=0.05,
        row_heights=[3, 1],
    )

    for col, workload in enumerate(workloads):
        for row, metric in enumerate(metrics):
            for allocator in allocators:
                data = (
                    df.filter(pl.col(ALLOCATOR) == allocator)
                    .filter(pl.col(WORKLOAD) == workload)
                    .select(THREAD_COUNT, metric)
                    .collect()
                )

                trace = go.Scatter(
                    x=data[THREAD_COUNT],
                    y=data[metric],
                    name=allocator,
                    legendgroup=allocator,
                    marker=dict(color=COLORS[allocator]),
                )

                fig.add_trace(trace, row=row + 1, col=col + 1)

    # Fix up axes
    fig.update_xaxes(range=(0, thread_counts[-1]))

    fig.for_each_yaxis(lambda yaxis: yaxis.update(type="log"), row=1)

    # Clip lightning RSS
    for col, workload in enumerate(workloads):
        data = (
            df.filter(pl.col(WORKLOAD) == workload)
            .select(MAX_RSS)
            .sort(MAX_RSS)
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

    fig.for_each_xaxis(lambda xaxis: xaxis.update(title="Thread Count"), row=2, col=1)

    for row, metric in enumerate(metrics):
        fig.for_each_yaxis(
            lambda yaxis: yaxis.update(title=metric),
            col=1,
            row=row + 1,
        )

    # Shade in NUMA
    fig.add_vrect(
        type="rect",
        x0=40,
        x1=80,
        line_width=0,
        fillcolor="black",
        opacity=0.10,
    )

    unique = set()
    # https://stackoverflow.com/a/62162555
    fig.for_each_trace(
        lambda trace: trace.update(showlegend=False)
        if (trace.name in unique)
        else unique.add(trace.name)
    )

    fig.update_layout(
        title="In Memory Key Value Store Workloads",
        width=1200,
        height=400,
        legend=dict(
            title_text=ALLOCATOR,
            orientation="h",
            xanchor="right",
            yanchor="top",
            y=-0.08,
            x=1.0,
        ),
        template=THEME,
        margin=dict(l=0, r=0, t=50, b=0),
    )

    fig.update_layout()
    fig.write_image("out.pdf")
    fig.show()


if __name__ == "__main__":
    main()
