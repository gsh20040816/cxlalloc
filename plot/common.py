import math
import polars as pl
import plotly
import plotly.io as pio


# https://github.com/plotly/plotly.py/issues/3469
pio.kaleido.scope.mathjax = None

ALLOCATOR = "Allocator"
THREAD_COUNT = "Thread Count"
WORKLOAD = "workload"
THROUGHPUT = "Throughput (ops/sec)"
MAX_RSS = "Max RSS (GiB)"
SCHEME = plotly.colors.qualitative.D3
THEME = "plotly_white"

ALLOCATORS = ["cxlalloc", "mimalloc", "ralloc", "cxl_shm", "boost", "lightning"]

COLORS = {
    "cxlalloc": "black",
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


def marker(allocator: str):
    return dict(color=COLORS[allocator], symbol=SYMBOLS[allocator], size=8)


def zorder(allocator: str):
    return len(ALLOCATORS) - ALLOCATORS.index(allocator)


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
