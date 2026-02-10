// (C) 2001-2024 Intel Corporation. All rights reserved.
// Your use of Intel Corporation's design tools, logic functions and other 
// software and tools, and its AMPP partner logic functions, and any output 
// files from any of the foregoing (including device programming or simulation 
// files), and any associated documentation or information are expressly subject 
// to the terms and conditions of the Intel Program License Subscription 
// Agreement, Intel FPGA IP License Agreement, or other applicable 
// license agreement, including, without limitation, that your use is for the 
// sole purpose of programming logic devices manufactured by Intel and sold by 
// Intel or its authorized distributors.  Please refer to the applicable 
// agreement for further details.


// Copyright 2022 Intel Corporation.
//
// THIS SOFTWARE MAY CONTAIN PREPRODUCTION CODE AND IS PROVIDED BY THE
// COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR
// BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
// OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE,
// EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
///////////////////////////////////////////////////////////////////////
// Creation Date : Feb, 2023
// Description   : SBCNT/DBCNT 

package mc_ecc_pkg;

 
//-------------------------
//------ Dev Mem Interfaces
//-------------------------
typedef struct packed {
    logic [7:0]                      SBE;
    logic [7:0]                      DBE;
    logic                            Valid;
} mc_rddata_ecc_t;

localparam  CL_ADDR_MSB = 51;
localparam  CL_ADDR_LSB = 6;
typedef logic [CL_ADDR_MSB:CL_ADDR_LSB]        Cl_Addr_t;

typedef struct packed {
    logic [255:0]   Data1;
    logic [255:0]   Data0;
} DataCL_t;

typedef struct packed {
    mc_rddata_ecc_t                  RdDataECC;
    logic                            RdDataValid;
} mc_devmem_if_t;

typedef struct packed {
    Cl_Addr_t                        DevAddr;
    logic [32:0]                     SBECnt;
    logic [32:0]                     DBECnt;
    logic [32:0]                     PoisonRtnCnt;
    logic                            NewSBE;
    logic                            NewDBE;
    logic                            NewPoisonRtn;
    logic                            NewPartialWr;
} mc_err_cnt_t;

localparam MC_ERR_CNT_WIDTH = $bits(mc_err_cnt_t); //149;
  

endpackage : mc_ecc_pkg
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwAprUhtzbqp6pOJ8jeCd70303wsq+cAZKUXhLuWRjcwuXx2rp4fDvUZCZRSgHwlTS+C5A+IfDeQrz6dvMUpQIW8Tk/0pL6SxoteYwSri/4lbVpsGxTWoz/v/8OmurUqT8/y/53WBSuGalBKrG4wNFSSnGYfNDUBNvtI2JmHJ3X5J1gDoubeKvzNc8sww+sP8Qy4utHuNO/zA3nmsl9Sz/XH4XtisVs0VjnksPVgGkIy6kNsDIx9cQIE2viwa/S8PbIVpM+BMwzF0E3XuZFdB43h8JSPy+9nVon1kWeO2CYUaogyZlZyRDHc2C9JdZ61Lf966WV21D/nSxYdQYYD79XDW7goG3EH22hrU9mxDnOYSR1pPWiUHt6x1kCan5owfuTkpOvHmDRbH+l+u6MlnX8OK1F3/pEHfBM2EPwT8xC7qJqnhnInnKXlb+mtqxLveNmtTNIHMP+m+R3etZ3Ix7TvLB34D8jsmHu27Z/1BANk6ytIh0/Mnmu2T/YKpGFKcqw8r7zX6qpSaNFrMMCzdELGaD8e+7jlnbgD55ZL9uMwYlyRtVmdXsFAvO+okDyeqgqkgRaRfaVZzQSmQ03rAcHyLSvA4S8Mi4jkmbYBeUAsCBLZtZmYExc9NK8s9jMq3RmYzFK4EzNFEwhmztGn0BIE/zGriaONVSHnArgfYFx0V0Oo+h7lWAcsjsYgcpDohJPeW9b/Kpg/bNqIIPAvie4I0bKTd1513+NC4GzSyp6Wp7Ir8OCdOL23aJD1oyjTWOzRMeDOdOFnWEJGXEP7bkA+"
`endif