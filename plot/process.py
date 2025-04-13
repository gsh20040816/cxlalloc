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
    df = pl.scan_ndjson(sys.argv[1], infer_schema_length=None)

    df = (
        common.collapse(
            df.filter(pl.col("allocator").struct["numa"].struct["policy"] == "bind"),
            common.MICRO_SELECT,
        )
        .group_by(cs.exclude(DATE, PROCESS_COUNT, THROUGHPUT, MAX_RSS))
        .agg(THROUGHPUT, MAX_RSS)
        .with_columns(
            mean=pl.col(THROUGHPUT).list.mean(),
            std=pl.col(THROUGHPUT).list.std() / pl.col(THROUGHPUT).list.mean() * 100,
        )
        .sort("std")
        .collect()
    )

    print(df)


if __name__ == "__main__":
    main()
