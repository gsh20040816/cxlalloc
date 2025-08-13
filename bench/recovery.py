import glob
import os
import subprocess as sp

# https://stackoverflow.com/questions/5137497/find-the-current-directory-and-files-directory
ROOT = os.path.dirname(os.path.realpath(__file__))
OBJECT_COUNT = 100000
CRASH_COUNTS = [0, 1, 2]
WORKLOADS = ["queue", "clevel"]
ITERATIONS = 10


def main():
    compile("ralloc")

    for block in [True, False]:
        for workload in WORKLOADS:
            for crash_count in CRASH_COUNTS:
                run("ralloc", workload, block, crash_count, 36)

    compile("cxlalloc")

    for workload in WORKLOADS:
        for crash_count in CRASH_COUNTS:
            run("cxlalloc", workload, False, crash_count, 36)


def run(allocator: str, workload: str, block: bool, crash_count: int, heap_size: int):
    for i in range(ITERATIONS):
        for path in glob.glob("/dev/shm/pool*"):
            os.remove(path)

        print(
            f"Running {allocator}, {workload}, block={block}, count={crash_count}, size={heap_size} ({i + 1}/{ITERATIONS})"
        )
        output = sp.run(
            [
                "env",
                "CXL_NUMA_NODE=1",
                "numactl",
                "--cpunodebind=0",
                "--membind=0",
                # "/usr/bin/time",
                # "-f",
                # "%E %M %U %S %F %R",
                f"{ROOT}/../target/release/cxlalloc-recover",
                "--workload",
                workload,
                "--crash-victim",
                "40",
                "--crash-count",
                str(crash_count),
                "--object-count",
                str(OBJECT_COUNT),
                "--path",
                "/dev/shm/pool",
                "--thread-count",
                "40",
                *(["--block"] if block else []),
                "--heap-size",
                str(2**heap_size),
            ],
            stdout=sp.PIPE,
            text=True,
        )

        with open(
            "recover.ndjson",
            "a",
        ) as file:
            file.write(output.stdout)
            file.write("\n")


def compile(allocator: str):
    args = [
        "cargo",
        "build",
        "--release",
        "--package",
        "cxlalloc-recover",
    ]

    if allocator == "cxlalloc":
        args.append("--features")
        args.append("cxlalloc-recover/cxlalloc")

    sp.run(args)


if __name__ == "__main__":
    main()
