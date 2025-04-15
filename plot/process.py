import common
from common import (
    ALLOCATOR,
    DATE,
    THROUGHPUT,
    MAX_RSS,
    PROCESS_COUNT,
    THREAD_COUNT,
    WORKLOAD,
)
import sys
import polars as pl
import polars.selectors as cs
import plotly.express as px


def main():
    df = common.scan_ndjson()

    df = (
        common.collapse(
            # df.filter(pl.col("allocator").struct["numa"].struct["policy"] == "bind"),
            df,
            workloads=common.HUGE_WORKLOADS,
        )
        # .group_by(cs.exclude(PROCESS_COUNT, THROUGHPUT, MAX_RSS))
        # .agg(THROUGHPUT, MAX_RSS)
        # .with_columns(
        #     mean=pl.col(THROUGHPUT).list.mean(),
        #     std_percent=pl.col(THROUGHPUT).list.std()
        #     / pl.col(THROUGHPUT).list.mean()
        #     * 100,
        # )
        # .sort("std_percent")
        .collect()
    )

    fig = px.line(
        df,
        x=THREAD_COUNT,
        y=THROUGHPUT,
        error_y=THROUGHPUT + "_std",
        color=PROCESS_COUNT,
        facet_col=WORKLOAD,
        markers=True,
        log_y=False,
    )
    fig.show()
    fig.write_image("out.pdf")


if __name__ == "__main__":
    main()
