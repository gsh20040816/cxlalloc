# mCAS Artiface Evaluation
This document oulines the steps to reproduce the mCAS related experiments. This includes
* Platform setup
* Running the microbenchmark
* Plotting the results

The files is organized into the following structure:
* `./rtl.zip` holds all the hardware related files.
* `./bitstreams` holds the FPGA configuration files.
* `./kmods` holds all the kernel modules.
* `./setup` holds all the setup scripts for configurating the device.
* `./ubench` holds all the c/cpp code for the microbenchmark.

## Before we begin
### Platform 
The test was conducted on the following machine:
* Intel(R) Xeon(R) Gold 6438Y+
* Altera Agilex 7 I-series FPGA, **board R1BES**
  * The RTL may be synthesized to board RBES or later version of the FPGA, but the provided bitstream must be tested on **board R1BES**.
* Ubuntu 24.04, Linux kernel v6.11

### Hardware setup
Please following Intel Agilex I-series handbook to program the FPGA. 
We provide the `.cdf` and `.pof` file for configurating the device, and we outline the steps below:
1. Connect the FPGA to the system via PCIe x16 slot and PCIe 8+2 pin power from the motherboard.
2. Either connect the micro-USB slot or the JTAG USB-blaster to program the FPGA. The programming machine can be the same as the testing machine.
3. Once the machine is setup, please upload the `.cdf` and `.pof` file to the testing machine.
4. Please install the programming toolkit from Altera's website: [Quartus Prime Pro Edition Programmer and Tools](https://www.altera.com/download-center/license-agreement/78691/358416d8d321d6b68fdc704508aa0b7d68a84a9a?filename=QuartusProProgrammerSetup-25.3.0.109-linux.run).
5. Once installing the programmign toolkit, the following command will program the FPGA with the `.cdf` and `.pof` file:
```
cd <path to quartus>/quartus/bin/
./quartus_pgm -c <usb-blaster name> <path to .cdf>
```
6. Finally, power cycle the machine to apply the bitstream changes to the FPGA.

The hardware RTL and Quartus proejct is zipped within the `./rtl.zip`. Synthesizing the RTL requires a Quartus Prime Pro edition license, as well as a CXL-Type2 License.

### Software setup
To use the FPGA as a CXL node, the following grub command needs to be in place:
```
efi=nosoftreserve
```
The system should have the following configuration:
```
available: 2 nodes (0-1)
node 0 cpus: 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
node 0 size: 128604 MB
node 0 free: 125531 MB
node 1 cpus:
node 1 size: 16062 MB
node 1 free: 16062 MB
node distances:
node   0   1
  0:  10  14
  1:  14  10
```

To find the PCIe BAR number for the CXL device, please run the following command:
```
$ sudo lspci | grep 0ddb
43:00.0 Processing accelerators: Intel Corporation Device 0ddb (rev 02)
43:00.1 Processing accelerators: Intel Corporation Device 0ddb (rev 02)
```
In this case, the BAR number to control the device is `43:00.1`.
Please update the bash variable in the `./setup/setup_all.sh` 
```
PCIE_BAR="43:00.1"
```

Once the machine recongnizes the CXL device, please head to `./setup` and run the following command to perform the necessary system configuration:
```
$ bash setup_all.sh
```

This script will 1) enable MMIO configuration for the PCIe device, 2) install the kernel module that exposes a contigous virtual address from the kernel space to the user space and retrive its physical address at the same time.

---
Here are the list of software(s) to install:
#### HdrHistogram_c
```
git clone https://github.com/HdrHistogram/HdrHistogram_c.git
cd HdrHistogram_c
cmake .
make
sudo make install
```

## Testing
### Microbenchmark
The project can be compiled with the following commands:
```
cd ubench
mkdir build
cd build
cmake ..
make -j
```

To reproduce the data, please run the following command:
```
cd ubench
bash test_ubench.sh
