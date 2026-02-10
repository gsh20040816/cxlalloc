#include <stdio.h>
#include <sys/mman.h>
#include <fcntl.h>
#include <sys/types.h>
#include <unistd.h>
#include <stdarg.h>
#include "util.h"
#include "csr.h"
#include <math.h>
#include <vector>

#include <fstream>
#include <immintrin.h>

/**
 * init_csr
 *   @brief Open the PCIe file and map to virtual memory
 *   @param pci_fd the pointer to the variable that will take the return value of fd
 *   @param pci_vaddr the pointer to the variable that will take the return value of mapped virtual address
 *   @return 0 means succedded, -1 means failed
 */
int init_csr(int *pci_fd, uint64_t **pci_vaddr) {

    uint64_t *ptr;
    int fd;

    fd = open(CXL_PCIE_BAR_PATH, O_RDWR | O_SYNC);
    if(fd == -1){
        LOG_ERROR(" Open BAR2 failed.\n");
        return -1;
    }
    LOG_INFO(" PCIe File opened.\n");

    ptr = (uint64_t*)mmap(0, (1 << 21), PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);     // 2MB
    if(ptr == (void *) -1){
        LOG_ERROR(" PCIe Device mmap not successful. Found -1.\n");
        close(fd);
        return -1;
    }
    if(ptr == (void *) 0){
        LOG_ERROR(" PCIe Device mmap not successful. Found 0.\n");
        close(fd);
        return -1;
    }

    LOG_INFO(" PCIe Device mmap succeeded.\n");
    LOG_INFO(" PCIe Memory mapped to address 0x%016lx.\n", (unsigned long) ptr);

    *pci_fd = fd;
    *pci_vaddr = ptr;

    return 0;
}

uint64_t access_cnt_to_MB(uint64_t cnt, uint64_t clk_tick, uint64_t clk_rate) {
    uint64_t ret;
    if (clk_tick == 0) return 0;
    ret = (cnt * 64 * clk_rate / clk_tick / 10000) >> 20;
    return ret; 
}

/**
 * clean_csr
 *   @brief Close the PCIe file and unmap the virtual memory
 *   @param pci_fd  the opened PCIe file
 *   @param pci_vaddr the mapped virtual address
 *   @return 0 means succedded, -1 means failed
 */
int clean_csr(int pci_fd, uint64_t *pci_vaddr) {

    int ret;
    LOG_DEBUG("clear csr\n");
    
    ret = munmap(pci_vaddr, 4096); 
    if (ret < 0) {
        LOG_ERROR(" mummap not successful.\n");
        return -1;
    }
    close(pci_fd);
    
    return 0;
}

