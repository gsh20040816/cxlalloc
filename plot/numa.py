import common
from common import ALLOCATOR, THROUGHPUT, MAX_RSS, THREAD_COUNT, WORKLOAD
import sys
import polars as pl
import polars.selectors as cs
import plotly.express as px


def main():
    df = pl.scan_ndjson(sys.argv[1], infer_schema_length=None)

    df = (
        common.collapse(
            df,
            common.MACRO_SELECT,
            pl.col("allocator").struct["numa"].struct["policy"].first(),
        )
        .collect()
        .pivot("policy", values=cs.by_name("date", THROUGHPUT, MAX_RSS))
        .select(
            ~(cs.starts_with(THROUGHPUT, MAX_RSS, "date")),
            (pl.col(THROUGHPUT + "_bind") / pl.col(THROUGHPUT + "_interleave")).alias(
                THROUGHPUT
            ),
            (pl.col(MAX_RSS + "_bind") / pl.col(MAX_RSS + "_interleave")).alias(
                MAX_RSS
            ),
        )
    )

    fig = px.line(
        df,
        x=THREAD_COUNT,
        y=THROUGHPUT,
        # color="policy",
        facet_row=ALLOCATOR,
        facet_col=WORKLOAD,
    )
    fig.for_each_yaxis(
        lambda yaxis: yaxis.update(
            title_text="Relative Throughput (bind / interleave)"
        ),
        col=1,
    )
    fig.show()

    # lo = df.select(pl.col(THROUGHPUT).arg_min()).item()
    # hi = df.select(pl.col(THROUGHPUT).arg_max()).item()
    #
    # print(df.row(lo))
    # print(df.row(hi))


if __name__ == "__main__":
    main()
