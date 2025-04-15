import enum
import math
import polars as pl
import polars.selectors as cs
import plotly
import plotly.io as pio
import sys

# https://github.com/plotly/plotly.py/issues/3469
pio.kaleido.scope.mathjax = None

# Columns
DATE = "date"
ALLOCATOR = "Allocator"
THREAD_COUNT = "Thread Count"
PROCESS_COUNT = "Process Count"
WORKLOAD = "workload"
THROUGHPUT = "Throughput (ops/sec)"
MAX_RSS = "Max RSS (GiB)"
METRICS = [THROUGHPUT, MAX_RSS]


class Allocator(enum.StrEnum):
    SHMALLOC = "shmalloc"
    SHMALLOC_CXL = "shmalloc-cxl"
    SHMALLOC_SFENCE = "shmalloc-sfence"
    SHMALLOC_CLFLUSHOPT = "shmalloc-clflushopt"
    MIMALLOC = "mimalloc"
    RALLOC = "ralloc"
    CXL_SHM = "cxl-shm"
    BOOST = "boost"
    LIGHTNING = "lightning"


_NAME = pl.col("allocator").struct["name"]
ALLOCATORS = {
    Allocator.SHMALLOC: (_NAME == "cxlalloc")
    & (pl.col("allocator").struct["consistency"] == "none")
    & (pl.col("allocator").struct["numa"].struct["node"] == 0),
    Allocator.SHMALLOC_CXL: (_NAME == "cxlalloc")
    & (pl.col("allocator").struct["consistency"] == "none")
    & (pl.col("allocator").struct["numa"].struct["node"] == 2),
    Allocator.SHMALLOC_SFENCE: (_NAME == "cxlalloc")
    & (pl.col("allocator").struct["consistency"] == "sfence"),
    Allocator.SHMALLOC_CLFLUSHOPT: (_NAME == "cxlalloc")
    & (pl.col("allocator").struct["consistency"] == "clflushopt"),
    Allocator.MIMALLOC: _NAME == "mimalloc",
    Allocator.RALLOC: _NAME == "ralloc",
    Allocator.CXL_SHM: _NAME == "cxl_shm",
    Allocator.BOOST: _NAME == "boost",
    Allocator.LIGHTNING: _NAME == "lightning",
}


class Workload(enum.StrEnum):
    MC_12 = "MC-12"
    MC_15 = "MC-15"
    MC_31 = "MC-31"
    MC_37 = "MC-37"

    YCSB_LOAD = "YCSB-Load"
    YCSB_A = "YCSB-A"
    YCSB_D = "YCSB-D"

    THREADTEST_SMALL = "threadtest-small"
    THREADTEST_LARGE = "threadtest-large"
    THREADTEST_HUGE = "threadtest-huge"

    XMALLOC_SMALL = "xmalloc-small"
    XMALLOC_HUGE = "xmalloc-huge"


_NAME = pl.col("benchmark").struct["name"]
WORKLOADS = {
    # memcached
    Workload.MC_12: (_NAME == "memcached")
    & pl.col("benchmark").struct["trace"].str.contains("12"),
    Workload.MC_15: (_NAME == "memcached")
    & pl.col("benchmark").struct["trace"].str.contains("15"),
    Workload.MC_31: (_NAME == "memcached")
    & pl.col("benchmark").struct["trace"].str.contains("31"),
    Workload.MC_37: (_NAME == "memcached")
    & pl.col("benchmark").struct["trace"].str.contains("37"),
    # ycsb
    Workload.YCSB_LOAD: _NAME == "ycsb-load",
    Workload.YCSB_A: (_NAME == "ycsb-run")
    & (pl.col("benchmark").struct["insert_proportion"] > 0.06),
    Workload.YCSB_D: (_NAME == "ycsb-run")
    & (pl.col("benchmark").struct["insert_proportion"] < 0.06),
    # threadtest
    Workload.THREADTEST_SMALL: (_NAME == "tt")
    & (pl.col("benchmark").struct["object_size"] == 8),
    Workload.THREADTEST_LARGE: (_NAME == "tt")
    & (pl.col("benchmark").struct["object_size"] == 1 << 15),
    Workload.THREADTEST_HUGE: (_NAME == "tt")
    & (pl.col("benchmark").struct["object_size"] == 1 << 30),
    # xmalloc
    Workload.XMALLOC_SMALL: (_NAME == "xm")
    & (pl.col("benchmark").struct["batch_count"] == 120),
    Workload.XMALLOC_HUGE: (_NAME == "xm") & (pl.col("benchmark").struct["huge"]),
}

MICRO_WORKLOADS = [
    Workload.THREADTEST_SMALL,
    Workload.THREADTEST_LARGE,
    Workload.XMALLOC_SMALL,
]
MACRO_WORKLOADS = [
    Workload.MC_12,
    Workload.MC_15,
    Workload.MC_31,
    Workload.MC_37,
    Workload.YCSB_LOAD,
    Workload.YCSB_A,
    Workload.YCSB_D,
]
HUGE_WORKLOADS = [Workload.THREADTEST_HUGE, Workload.XMALLOC_HUGE]

# Theming
SCHEME = plotly.colors.qualitative.D3
THEME = "plotly_white"

SIZE_SUBPLOT_TITLE = 16
SIZE_YAXIS_TITLE = 16
SIZE_XAXIS_TITLE = 16
SIZE_LEGEND_TITLE = 16
SIZE_LEGEND_ENTRY = 16

COLORS = {
    Allocator.SHMALLOC: "black",
    Allocator.SHMALLOC_CXL: "black",
    Allocator.SHMALLOC_SFENCE: "black",
    Allocator.SHMALLOC_CLFLUSHOPT: "black",
    Allocator.MIMALLOC: SCHEME[0],
    Allocator.RALLOC: SCHEME[1],
    Allocator.CXL_SHM: SCHEME[2],
    Allocator.BOOST: SCHEME[3],
    Allocator.LIGHTNING: SCHEME[4],
}

# https://plotly.com/python-api-reference/generated/plotly.graph_objects.Scatter.html#plotly.graph_objects.scatter.Line.dash
DASHES = {
    Allocator.SHMALLOC: "solid",
    Allocator.SHMALLOC_CXL: "solid",
    Allocator.SHMALLOC_SFENCE: "solid",
    Allocator.SHMALLOC_CLFLUSHOPT: "solid",
    Allocator.MIMALLOC: "solid",
    Allocator.RALLOC: "solid",
    Allocator.CXL_SHM: "solid",
    Allocator.BOOST: "solid",
    Allocator.LIGHTNING: "solid",
}

SYMBOLS = {
    Allocator.SHMALLOC: "circle",
    Allocator.MIMALLOC: "triangle-up",
    Allocator.RALLOC: "square",
    Allocator.CXL_SHM: "diamond",
    Allocator.BOOST: "cross",
    Allocator.LIGHTNING: "x",
    Allocator.SHMALLOC_CXL: "square",
    Allocator.SHMALLOC_SFENCE: "diamond",
    Allocator.SHMALLOC_CLFLUSHOPT: "cross",
}


def scan_ndjson(paths: [str] = sys.argv[1:]):
    return pl.scan_ndjson(paths, infer_schema_length=None)


def update_layout(fig, full: bool, numa: bool, **kwargs):
    # Deduplicate legend entries
    # https://stackoverflow.com/a/62162555
    unique = set()
    fig.for_each_trace(
        lambda trace: trace.update(showlegend=False)
        if (trace.name in unique)
        else unique.add(trace.name)
    )

    # Update subplot title sizes
    # https://community.plotly.com/t/setting-subplot-title-font-sizes/46612/2
    fig.update_annotations(font_size=SIZE_SUBPLOT_TITLE)

    fig.for_each_xaxis(
        lambda xaxis: xaxis.update(
            title=dict(text="Thread Count", font_size=SIZE_XAXIS_TITLE)
        ),
        row=2,
        col=1 if full else None,
    )

    for row, metric in enumerate(METRICS):
        fig.for_each_yaxis(
            lambda yaxis: yaxis.update(
                title=dict(text=metric, font_size=SIZE_YAXIS_TITLE),
            ),
            col=1,
            row=row + 1,
        )

    # Shade in NUMA
    if numa:
        fig.add_vrect(
            type="rect",
            x0=40,
            x1=80,
            line_width=0,
            fillcolor="black",
            opacity=0.10,
        )

    fig.update_layout(
        width=1200 if full else 600,
        height=400,
        legend=dict(
            title=dict(text=ALLOCATOR, font_size=SIZE_LEGEND_TITLE),
            orientation="h",
            xanchor="right" if full else "left",
            yanchor="top",
            font_size=SIZE_LEGEND_ENTRY,
            y=-0.08 if full else -0.16,
            x=1.0 if full else 0,
        ),
        template=THEME,
        margin=dict(l=0, r=0, t=20, b=0),
        **kwargs,
    )


def style(allocator, function, *args, **kwargs):
    return function(
        *args,
        name=allocator,
        legendgroup=allocator,
        line=dict(color=COLORS[allocator], dash=DASHES[allocator]),
        marker=dict(color=COLORS[allocator], symbol=SYMBOLS[allocator], size=8),
        zorder=len(ALLOCATORS) - list(ALLOCATORS.keys()).index(allocator),
        **kwargs,
    )


def collapse(
    df, allocators=list(ALLOCATORS.keys()), workloads=list(WORKLOADS.keys()), *agg
):
    allocators = translate(ALLOCATORS, allocators)
    workloads = translate(WORKLOADS, workloads)

    return (
        df.group_by("date")
        .agg(
            allocators.first().alias(ALLOCATOR),
            pl.col("global").struct["process_count"].first().alias(PROCESS_COUNT),
            pl.col("global").struct["thread_count"].first().alias(THREAD_COUNT),
            workloads.first().alias(WORKLOAD),
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
            *agg,
        )
        .drop(DATE)
        .group_by(cs.exclude(THROUGHPUT, MAX_RSS))
        .agg(
            pl.col(THROUGHPUT).mean().alias(THROUGHPUT),
            pl.col(THROUGHPUT).std().alias(THROUGHPUT + "_std"),
            pl.col(MAX_RSS).mean().alias(MAX_RSS),
            pl.col(MAX_RSS).std().alias(MAX_RSS + "_std"),
        )
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
        .sort(WORKLOAD, ALLOCATOR, PROCESS_COUNT, THREAD_COUNT)
    )


def translate(translate: dict[str, pl.Expr], keys: [str]) -> pl.Expr:
    expr = pl.when(translate[keys[0]]).then(pl.lit(keys[0]))
    for workload in keys[1:]:
        expr = expr.when(translate[workload]).then(pl.lit(workload))
    return expr


def display_count(value: int) -> str:
    suffixes = ["", "K", "M", "B", "T"]
    if value == 0:
        return "0"

    index = int(math.log10(value) / 3)
    if index == 0:
        return f"{value}"
    else:
        return f"{value / (10 ** (3 * index)):.01f}{suffixes[index]}"


def display_size(value: int) -> str:
    suffixes = ["B", "KiB", "MiB", "GiB"]
    if value == 0:
        return ""

    index = int(math.log2(value) / 10)
    if index == 0:
        return f"{value}"
    else:
        return f"{value / (2 ** (10 * index)):.01f}{suffixes[index]}"


# https://github.com/pola-rs/polars/issues/12353
def unnest_all(df, separator="/"):
    def recurse(columns, namespace, selector):
        select = pl.col if selector is None else lambda col: selector.struct.field(col)

        for col, dtype in columns.items():
            name = col if namespace == "" else f"{namespace}/{col}"

            if hasattr(dtype, "fields"):
                yield from recurse(
                    {field.name: field.dtype for field in dtype.fields},
                    name,
                    select(col),
                )
            # FIXME: only supports lists of structs, which
            # is true in our case (`output/thread`)
            elif hasattr(dtype, "inner"):
                yield from recurse(
                    {field.name: field.dtype for field in dtype.inner.fields},
                    name,
                    select(col).list.explode(),
                )
            else:
                yield name, select(col).alias(name)

    return {name: selector for name, selector in recurse(df.collect_schema(), "", None)}
