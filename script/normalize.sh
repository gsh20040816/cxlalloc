#!/usr/bin/env bash
# https://github.com/MoatLab/Pond/blob/master/cxl-global.sh
# https://easyperf.net/blog/2019/08/02/Perf-measurement-environment-on-Linux

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

# readonly cxl_numa_node=${CXL_NUMA_NODE}
# readonly cpu_numa_node=${CPU_NUMA_NODE}

readonly kernel="/proc/sys/kernel";

# Allow perf without root
echo '-1' | sudo tee $kernel/perf_event_paranoid

# Include kernel symbols in flamegraph
echo 0 | sudo tee $kernel/kptr_restrict

# Disable NMI watchdog
# https://docs.kernel.org/admin-guide/lockup-watchdogs.html
echo 0 | sudo tee $kernel/nmi_watchdog

# Disable NUMA balancing
# https://docs.kernel.org/admin-guide/sysctl/kernel.html#numa-balancing
echo 0 | sudo tee $kernel/numa_balancing

# https://docs.kernel.org/admin-guide/mm/ksm.html
echo 0 | sudo tee /sys/kernel/mm/ksm/run

readonly system=/sys/devices/system
readonly cpu=$system/cpu

# CPU settings: https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-devices-system-cpu

# Disable turbo boost
# https://en.wikipedia.org/wiki/Intel_Turbo_Boost
#
# Note: usually Intel CPUs use the intel_pstate scaling driver,
# but the SPR2 machine we're benchmarking on boots with:
#
# > intel_pstate: CPU model not supported
#
# https://www.kernel.org/doc/html/v5.0/admin-guide/pm/intel_pstate.html
if test -d "$cpu/intel_pstate"; then
    echo 1 | sudo tee $cpu/intel_pstate/no_turbo
else
    echo 0 | sudo tee $cpu/cpufreq/boost
fi

# Set performance mode
echo "performance" | sudo tee $cpu/cpu*{0..9}/cpufreq/scaling_governor

# Disable SMT
# https://en.wikipedia.org/wiki/Simultaneous_multithreading
if test -d "$cpu/smt"; then
    echo off | sudo tee $cpu/smt/control >/dev/null 2>&1
fi

# Disable CPUs on NUMA node 0
# echo 0 | sudo tee ${system}/node/node${cxl_numa_node}/cpu*{0..9}*/online

# Set up CXL device
# if [[ $(daxctl list) ]]; then
#     sudo daxctl reconfigure-device --mode=system-ram dax0.0 --force
# fi

# Restrict system threads
# https://documentation.suse.com/sle-rt/15-SP6/html/SLE-RT-all/cha-shielding-with-systemd.html
# sudo systemctl set-property --runtime init.scope AllowedCPUs=0 AllowedMemoryNodes=0
# sudo systemctl set-property --runtime system.slice AllowedCPUs=0 AllowedMemoryNodes=0
# echo "[Slice]\nAllowedCpus=40-79" | sudo tee /etc/systemd/system/workload.slice
