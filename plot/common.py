import math
import polars as pl
import polars.selectors as cs
import plotly
import plotly.io as pio


# https://github.com/plotly/plotly.py/issues/3469
pio.kaleido.scope.mathjax = None

DATE = "date"
ALLOCATOR = "Allocator"
THREAD_COUNT = "Thread Count"
PROCESS_COUNT = "Process Count"
WORKLOAD = "workload"
THROUGHPUT = "Throughput (ops/sec)"
MAX_RSS = "Max RSS (GiB)"
SCHEME = plotly.colors.qualitative.D3
THEME = "plotly_white"

ALLOCATORS = [
    "cxlalloc",
    "cxlalloc-extend",
    "cxlalloc-sfence",
    "cxlalloc-clflushopt",
    "mimalloc",
    "ralloc",
    "cxl_shm",
    "boost",
    "lightning",
]

COLORS = {
    "cxlalloc": "black",
    "cxlalloc-extend": "black",
    "cxlalloc-sfence": "black",
    "cxlalloc-clflushopt": "black",
    "mimalloc": SCHEME[0],
    "ralloc": SCHEME[1],
    "cxl_shm": SCHEME[2],
    "boost": SCHEME[3],
    "lightning": SCHEME[4],
}

SYMBOLS = {
    "cxlalloc": "circle",
    "mimalloc": "triangle-up",
    "ralloc": "square",
    "cxl_shm": "diamond",
    "boost": "cross",
    "lightning": "x",
}

MACRO_SELECT = (
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
)

MACRO_WORKLOADS = ["YCSB-Load", "YCSB-A", "YCSB-D", "MC-12", "MC-15", "MC-31", "MC-37"]

MICRO_SELECT = (
    pl.when(pl.col("benchmark").struct["object_size"] == 8)
    .then(pl.lit("threadtest-8B"))
    .when(pl.col("benchmark").struct["object_size"] == 32768)
    .then(pl.lit("threadtest-32KiB"))
    .otherwise(pl.lit("xmalloc"))
)

MICRO_WORKLOADS = ["threadtest-8B", "threadtest-32KiB", "xmalloc"]


def marker(allocator: str):
    return dict(color=COLORS[allocator], symbol=SYMBOLS[allocator], size=8)


def zorder(allocator: str):
    return len(ALLOCATORS) - ALLOCATORS.index(allocator)


def collapse(df, workload, *agg):
    return (
        df.group_by("date")
        .agg(
            pl.col("allocator").struct["name"].first().alias(ALLOCATOR),
            pl.col("global").struct["process_count"].first().alias(PROCESS_COUNT),
            pl.col("global").struct["thread_count"].first().alias(THREAD_COUNT),
            workload.first().alias(WORKLOAD),
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
        .sort(ALLOCATOR, WORKLOAD, PROCESS_COUNT, THREAD_COUNT)
    )


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
