# r1bes_type2_2chan_24_3_mCAS
## Hardware
All design goes into the [./hardware_test_design/common/afu/mcas_ctrl.sv](./hardware_test_design/common/afu/mcas_ctrl.sv). 
The design targets performing mCAS on the channel 0 of memory, thus all mCAS
must be issued to even-indexed cachelines. 

## Overview
The hardware captures "special reads" and "special writes" in the iAFU. 
Then the hardware stores the information in the "special write", while 
converting the "special reads" into access into the mCAS address. 

The mCAS read responds is then converted into matching check
as well as the original value of the cacheline.

Upon this mCAS read responds, the hardware also performs a check to all
on-going mCAS requests, and force the ones with matching address to 
always return with a mismatch.

The procedure is fully pipelined and all operations is performed in line
rate of the memory requests. 
