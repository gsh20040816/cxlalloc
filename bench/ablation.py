import os
import subprocess as sp

# https://stackoverflow.com/questions/5137497/find-the-current-directory-and-files-directory
ROOT = os.path.dirname(os.path.realpath(__file__))


def main():
    compile(False)

    sp.run(
        [
            f"{ROOT}/../target/release/cxlalloc-bench",
            f"{ROOT}/../cxlalloc-bench/workloads/ablation-hwcc.toml",
        ]
    )

    compile(True)

    sp.run(
        [
            f"{ROOT}/../target/release/cxlalloc-bench",
            f"{ROOT}/../cxlalloc-bench/workloads/ablation-mcas.toml",
        ]
    )


def compile(mcas: bool):
    args = [
        "cargo",
        "build",
        "--release",
        "--package",
        "cxlalloc-bench",
        "--no-default-features",
        "--features",
        "allocator-mimalloc",
        "--features",
        "allocator-cxlalloc",
        "--features",
        "recover-shm",
    ]

    if mcas:
        args.append("--features")
        args.append("cxl-mcas")

    sp.run(args)


if __name__ == "__main__":
    main()
