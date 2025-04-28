import sys
import common
import polars as pl


def main():
    pl.Config.set_tbl_rows(-1)

    cxlalloc = pl.scan_ndjson(sys.argv[1]).filter(
        pl.col("allocator").struct["name"] == "cxlalloc"
    )
    mimalloc = pl.scan_ndjson(sys.argv[1]).filter(
        pl.col("allocator").struct["name"] == "mimalloc"
    )
    # cxl_shm = pl.scan_ndjson(sys.argv[2]).filter(
    #     pl.col("allocator").struct["name"] == "cxl_shm"
    # )

    cxlalloc = (
        common.collapse(cxlalloc)
        .drop_nulls()
        .select(
            common.PROCESS_COUNT,
            common.THREAD_COUNT,
            common.WORKLOAD,
            pl.col(common.THROUGHPUT).alias("tput_cxlalloc"),
        )
        # .filter(pl.col(common.WORKLOAD).str.starts_with("MC"))
        .filter(pl.col(common.WORKLOAD).str.starts_with("threadtest"))
    )

    mimalloc = (
        common.collapse(mimalloc)
        # .filter(pl.col(common.WORKLOAD).str.starts_with("MC"))
        .filter(pl.col(common.WORKLOAD).str.starts_with("threadtest"))
        .select(pl.col(common.THROUGHPUT).alias("tput_mimalloc"))
    )

    # cxl_shm = (
    #     common.collapse(cxl_shm)
    #     .filter(pl.col(common.WORKLOAD).str.starts_with("YCSB"))
    #     .select(pl.col(common.THROUGHPUT).alias("tput_cxl_shm"))
    # )

    df = (
        pl.concat(
            [cxlalloc, mimalloc],
            how="horizontal",
        )
        .with_columns(
            cxlalloc_over_mimalloc=pl.col("tput_cxlalloc") / pl.col("tput_mimalloc"),
            # cxl_shm_over_mimalloc=pl.col("tput_cxl_shm") / pl.col("tput_mimalloc"),
            # opt_over_nomemcpy=pl.col("tput_opt") / pl.col("tput_nomemcpy"),
        )
        .mean()
        .collect()
    )

    print(df)


if __name__ == "__main__":
    main()
