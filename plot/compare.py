import polars as pl
import common


def main():
    pl.Config.set_tbl_rows(-1)

    df = common.scan_ndjson()
    df = common.collapse(df)

    cxlalloc = df.filter(pl.col(common.ALLOCATOR) == pl.lit(common.Allocator.CXLALLOC))
    mimalloc = df.filter(
        pl.col(common.ALLOCATOR) == pl.lit(common.Allocator.MIMALLOC)
    ).select(pl.col(common.THROUGHPUT).alias("mimalloc"))

    print(
        "Average performance drop relative to mimalloc:",
        pl.concat([cxlalloc, mimalloc], how="horizontal")
        .with_columns(relative=pl.col(common.THROUGHPUT) / "mimalloc")
        # Switch filters as necessary
        .filter(pl.col(common.WORKLOAD) == common.Workload.YCSB_LOAD)
        .filter(pl.col(common.THREAD_COUNT) >= 40)
        .select((1.0 - pl.col("relative").mean()) * 100)
        .collect()
        .item(),
        "%",
    )

    ralloc = df.filter(
        pl.col(common.ALLOCATOR) == pl.lit(common.Allocator.RALLOC)
    ).select(pl.col(common.HWCC).alias("ralloc"))

    print(
        "Average HWcc usage relative to ralloc (all workloads):",
        pl.concat([cxlalloc, ralloc], how="horizontal")
        .filter(pl.col(common.WORKLOAD).is_in(common.MACRO_WORKLOADS))
        .select((pl.col(common.HWCC) / "ralloc").mean() * 100)
        .collect()
        .item(),
    )

    print(
        "Average HWcc usage relative to ralloc (working set > 8GiB):",
        pl.concat([cxlalloc, ralloc], how="horizontal")
        .filter(
            pl.col(common.WORKLOAD).is_in(
                [
                    common.Workload.YCSB_A,
                    common.Workload.YCSB_D,
                    common.Workload.YCSB_LOAD,
                    common.Workload.MC_12,
                ]
            )
        )
        .select(
            pl.col(common.WORKLOAD),
            pl.col(common.HWCC) * (2**10),
            (pl.col(common.HWCC) / pl.col(common.PSS) * 100)
            .mean()
            .alias("relative-pss"),
            (pl.col(common.HWCC) / "ralloc").mean().alias("relative-ralloc") * 100,
        )
        .collect(),
    )


if __name__ == "__main__":
    main()
