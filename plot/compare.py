import polars as pl
import common


def main():
    pl.Config.set_tbl_rows(-1)

    df = common.scan_ndjson()
    df = common.collapse(df)

    shm = df.filter(pl.col(common.ALLOCATOR) == pl.lit(common.Allocator.CXLALLOC))
    mi = df.filter(
        pl.col(common.ALLOCATOR) == pl.lit(common.Allocator.MIMALLOC)
    ).select(pl.col(common.THROUGHPUT).alias("mi"))

    print(
        "Average performance drop relative to mimalloc:",
        pl.concat([shm, mi], how="horizontal")
        .with_columns(relative=pl.col(common.THROUGHPUT) / "mi")
        # Switch filters as necessary
        .filter(pl.col(common.WORKLOAD) == common.Workload.YCSB_LOAD)
        .filter(pl.col(common.THREAD_COUNT) >= 40)
        .select((1.0 - pl.col("relative").mean()) * 100)
        .collect()
        .item(),
        "%",
    )


if __name__ == "__main__":
    main()
