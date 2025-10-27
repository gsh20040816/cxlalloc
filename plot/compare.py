import polars as pl
import common


def main():
    pl.Config.set_tbl_rows(-1)

    df = common.scan_ndjson()
    df = common.collapse(df)

    base = df.filter(pl.col(common.ALLOCATOR) == pl.lit(common.Allocator.CXLALLOC))

    # Throughput analysis
    throughput = pl.concat(
        [
            base,
            *[
                df.filter(pl.col(common.ALLOCATOR) == pl.lit(allocator)).select(
                    pl.col(common.THROUGHPUT).alias(allocator)
                )
                for allocator in [
                    common.Allocator.CXLALLOC,
                    common.Allocator.CXLALLOC_NONRECOVERABLE,
                    common.Allocator.MIMALLOC,
                    common.Allocator.RALLOC,
                ]
            ],
        ],
        how="horizontal",
    )

    for group in [
        common.MACRO_WORKLOADS,
        [common.Workload.THREADTEST_SMALL],
        [common.Workload.XMALLOC_SMALL],
    ]:
        print(
            f"Throughput comparisons ({group})",
            throughput
            # Switch filters as necessary
            .filter(pl.col(common.WORKLOAD).is_in(group))
            # .filter(pl.col(common.THREAD_COUNT) == 80)
            .select(
                *[
                    (pl.col(over) / pl.col(under))
                    .log()
                    .mean()
                    .exp()
                    .alias(f"{over}-{under}")
                    * 100
                    for over, under in [
                        (common.Allocator.CXLALLOC, common.Allocator.MIMALLOC),
                        (common.Allocator.RALLOC, common.Allocator.MIMALLOC),
                        (
                            common.Allocator.CXLALLOC,
                            common.Allocator.CXLALLOC_NONRECOVERABLE,
                        ),
                    ]
                ],
            )
            .collect(),
        )

    # HWcc analysis

    hwcc = pl.concat(
        [
            base,
            *[
                df.filter(pl.col(common.ALLOCATOR) == pl.lit(allocator)).select(
                    pl.col(common.HWCC).alias(allocator)
                )
                for allocator in [
                    common.Allocator.RALLOC,
                ]
            ],
        ],
        how="horizontal",
    )

    for group in [
        common.MACRO_WORKLOADS,
        [common.Workload.THREADTEST_SMALL],
        [common.Workload.XMALLOC_SMALL],
    ]:
        print(
            f"HWcc comparisons ({group}):",
            hwcc.filter(pl.col(common.WORKLOAD).is_in(group))
            .select(
                (pl.col(common.HWCC) / pl.col(common.PSS) * 100)
                .mean()
                .alias("relative-pss"),
                (pl.col(common.HWCC) / common.Allocator.RALLOC)
                .mean()
                .alias("relative-ralloc")
                * 100,
            )
            .collect(),
        )


if __name__ == "__main__":
    main()
