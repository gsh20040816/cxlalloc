#ifndef CSR_H
#define CSR_H

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#define CSR_MHZ 125000000
/*
 *  =================================
 *         MMIO for monitoring status
 *  =================================
 */
#define CSR_CLOCK 0
#define CSR_READ_CNT 1
#define CSR_WRITE_CNT 2

// #define CXL_PCIE_BAR_PATH  "/sys/devices/pci0000:40/0000:40:00.1/resource2"
//  #define CXL_PCIE_BAR_PATH  "/sys/devices/pci0000:5d/0000:5d:00.1/resource2"
#define CXL_PCIE_BAR_PATH "/sys/devices/pci0000:43/0000:43:00.1/resource2"

typedef struct fpga_counters {
  uint64_t clock;
  uint64_t read;
  uint64_t write;
  uint64_t rd_bw;
  uint64_t wr_bw;
  uint64_t queue_len;
  uint64_t push_cnt;
  uint64_t pfn_cnt;
} fpga_counters_t;

int init_csr(int *pci_fd, uint64_t **pci_vaddr);

int clean_csr(int pci_fd, uint64_t *pci_vaddr);

// static inline uint64_t rdtsc_start(void);
// static inline uint64_t rdtsc_end(void);

#endif // CSR_H
