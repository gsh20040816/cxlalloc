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


// Copyright 2023 Intel Corporation.
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
 /* structs for bit widths
  
    APRIL 14 2023 - these are set based on Darren's current CXL IP HAS draft ch3.3 values
 */

package ed_mc_axi_if_pkg;
  import cafu_common_pkg::*;  // import to use protocol specific enumuration structs
  import ed_cxlip_top_pkg::*;

// ================================================================================================
 /* structs for bit widths
  
    APRIL 14 2023 - these are set based on Darren's current CXL IP HAS draft ch3.3 values
 */
// @@copy for common_afu_pkg@@start
  localparam MC_AXI_WAC_REGION_BW  =  4; // awregion
  localparam MC_AXI_WAC_ADDR_BW    = 52; // awaddr  - using bits 51:6 of 64-bits, also grabbing the lower 6 bits?
  localparam MC_AXI_WAC_USER_BW    =  1; // awuser
  localparam MC_AXI_WAC_ID_BW      =  8; // awid    - feb2024 - changed from 12
  localparam MC_AXI_WAC_BLEN_BW    = 10; // awlen
  
  localparam MC_AXI_WDC_DATA_BW = 512; // wwdata
  localparam MC_AXI_WDC_USER_BW =  1;  // wuser  // currently only poison
  
  localparam MC_AXI_WDC_STRB_BW = MC_AXI_WDC_DATA_BW / 8; // wstrb
  
  localparam MC_AXI_WRC_ID_BW   =  8; // bid   - feb2024 - changed from 12
  localparam MC_AXI_WRC_USER_BW =  1; // buser
  
  localparam MC_AXI_RAC_REGION_BW  =  4; // arregion
  localparam MC_AXI_RAC_ID_BW      =  8; // arid    - feb2024 - changed from 12
  localparam MC_AXI_RAC_ADDR_BW    = 52; // araddr  - using bits 51:6 of 64-bits, also grabbing the lower 6 bits?
  localparam MC_AXI_RAC_BLEN_BW    = 10; // arlen
  localparam MC_AXI_RAC_USER_BW    =  1; // aruser
  
  localparam MC_AXI_RRC_ID_BW        =   8; // rid   - feb2024 - changed from 12
  localparam MC_AXI_RRC_DATA_BW      = 512; // rdata
  localparam MC_EMIF_AMM_RRC_DATA_BW = 576; // rdata from EMIF AMM.

// ================================================================================================
// struct for read response channel response field
// ================================================================================================
  typedef struct packed {
	logic poison;
  } t_rd_rsp_user;

  localparam MC_AXI_RRC_USER_BW = $bits( t_rd_rsp_user );
  
// ================================================================================================
// AXI signals from BBS to MC
// ================================================================================================
  typedef struct packed {
    cafu_common_pkg::t_cafu_axi4_wr_resp_ready   bready;
    cafu_common_pkg::t_cafu_axi4_rd_resp_ready   rready;
	
	logic [MC_AXI_WAC_ID_BW-1:0]                 awid;
	logic [MC_AXI_WAC_ADDR_BW-1:0]               awaddr;
	logic [MC_AXI_WAC_BLEN_BW-1:0]               awlen;
	cafu_common_pkg::t_cafu_axi4_burst_size_encoding   awsize;
	cafu_common_pkg::t_cafu_axi4_burst_encoding        awburst;
	cafu_common_pkg::t_cafu_axi4_prot_encoding         awprot;
	cafu_common_pkg::t_cafu_axi4_qos_encoding          awqos;
	logic                                        awvalid;
	cafu_common_pkg::t_cafu_axi4_awcache_encoding      awcache;
	cafu_common_pkg::t_cafu_axi4_lock_encoding         awlock;
	logic [MC_AXI_WAC_REGION_BW-1:0]             awregion;
	logic [MC_AXI_WAC_USER_BW-1:0]               awuser;
	
    logic [MC_AXI_WDC_DATA_BW-1:0] wdata;
	logic [MC_AXI_WDC_STRB_BW-1:0] wstrb;
	logic                          wlast;
	logic                          wvalid;
	logic [MC_AXI_WDC_USER_BW-1:0] wuser; // currently only poison
	
	logic [MC_AXI_RAC_ID_BW-1:0]                 arid;
	logic [MC_AXI_RAC_ADDR_BW-1:0]               araddr;
	logic [MC_AXI_RAC_BLEN_BW-1:0]               arlen;
    cafu_common_pkg::t_cafu_axi4_burst_size_encoding   arsize;
    cafu_common_pkg::t_cafu_axi4_burst_encoding        arburst;
    cafu_common_pkg::t_cafu_axi4_prot_encoding         arprot;
    cafu_common_pkg::t_cafu_axi4_qos_encoding          arqos;
	logic                                        arvalid;
    cafu_common_pkg::t_cafu_axi4_arcache_encoding      arcache;
    cafu_common_pkg::t_cafu_axi4_lock_encoding         arlock;
    logic [MC_AXI_RAC_REGION_BW-1:0]             arregion;
    logic [MC_AXI_RAC_USER_BW-1:0]               aruser;
  } t_to_mc_axi4;
  
  localparam TO_MC_AXI4_BW = $bits(t_to_mc_axi4);
  
// ================================================================================================
  typedef struct packed {
    cafu_common_pkg::t_cafu_axi4_wr_addr_ready   awready;
    cafu_common_pkg::t_cafu_axi4_wr_data_ready    wready;
    cafu_common_pkg::t_cafu_axi4_rd_addr_ready   arready;
	
	logic [MC_AXI_WRC_ID_BW-1:0]           bid;
	cafu_common_pkg::t_cafu_axi4_resp_encoding   bresp;
	logic                                  bvalid;
	logic [MC_AXI_WRC_USER_BW-1:0]         buser;
	
	logic [MC_AXI_RRC_ID_BW-1:0]           rid;
	logic [MC_AXI_RRC_DATA_BW-1:0]         rdata;
	cafu_common_pkg::t_cafu_axi4_resp_encoding   rresp;
	logic                                  rvalid;
	logic                                  rlast;
    //logic [MC_AXI_RRC_USER_BW-1:0]         ruser;
	t_rd_rsp_user                          ruser;
  } t_from_mc_axi4;
  
  localparam FROM_MC_AXI4_BW = $bits(t_from_mc_axi4);
  localparam FROM_MC_AXI4_BW_PARM = $bits(t_from_mc_axi4);
// @@copy for common_afu_pkg@@end
  
// ================================================================================================
  typedef struct packed {
        cafu_common_pkg::t_cafu_axi4_rd_addr_ready   arready;
        logic                                  rd_id_fifo_almost_full;

        logic [MC_AXI_RRC_ID_BW-1:0]           rid;
        logic [MC_EMIF_AMM_RRC_DATA_BW-1:0]    rdata;
        cafu_common_pkg::t_cafu_axi4_resp_encoding   rresp;
        logic                                  rvalid;
        logic                                  rlast;
    //logic [MC_AXI_RRC_USER_BW-1:0]         ruser;
        t_rd_rsp_user                          ruser;
  } t_from_mc_axi4_rchan;

  localparam FROM_MC_AXI4_RCHAN_BW = $bits(t_from_mc_axi4_rchan);

// ================================================================================================
  typedef struct packed {

    cafu_common_pkg::t_cafu_axi4_wr_addr_ready   awready;
    cafu_common_pkg::t_cafu_axi4_wr_data_ready    wready;

        logic [MC_AXI_WRC_ID_BW-1:0]           bid;
        cafu_common_pkg::t_cafu_axi4_resp_encoding   bresp;
        logic                                  bvalid;
        logic [MC_AXI_WRC_USER_BW-1:0]         buser;
  } t_from_mc_axi4_bchan;

  localparam FROM_MC_AXI4_BCHAN_BW = $bits(t_from_mc_axi4_bchan);

  // ================================================================================================
  typedef struct packed {
    logic [MC_AXI_RRC_ID_BW-1:0]           rid;
    logic [MC_AXI_RRC_DATA_BW-1:0]         rdata;
    cafu_common_pkg::t_cafu_axi4_resp_encoding   rresp;
    logic                                  rvalid;
	  logic                                  rlast;
    t_rd_rsp_user                          ruser;
  } t_mc_rdrsp_axi4;

// ================================================================================================
endpackage : ed_mc_axi_if_pkg
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EknmHwQp1Im7e42VTEW4oP2YlkjZ4q1qxNf0pFfjUqCitHkH39CYcFkwSiDngDquknlwOs0cV09OWg1Vqr9AJzKHl9u3jmr1ART1KkS8wXukVlgHSVfd16n8/T9v5gAWwkVERtgG7ZXREFE2Ma4KnGT/aFGmA6j1B8XDnuCKFAsk/YDTkJMsrKEHO5zzdNk6r7SaThxY+E0ejgfrwMULALf2OnQu+lk4HNYpq3cSGSRmac8hENPZJWt4I+ZfvcRwXEaFl7uIyQjUvJ9OiRTV69SuwHKm/DLArYKy23c6TxazxTCH48Swqc14z+e0xug3EbETlWM94waSe1deLNoCjD3m2BzwwVsFLIHZg62PNTtRFpvPfIS8Rb11C3WKEkdof0T+2Y6vSw/EqIG0+p68HakZkdN/mbTLrlmBlcib7ku1vg2YqZ2sxkGxzoCY3+HF5LGhvDiVad8n9O6xSU3FseBBCZVi8WC5flGLjW/xnbSLbgOrGnp3+n6/BNjNXy1xjzrWR4ZhLQUKLrBM8aIK7lFrJ3Yv6kTOotfsDtXG/iUniFZFAR4A3pei3jSdRTSXID+6Lj+6dM6FD8OfiMPfuNAwUW6rm2/SM4Mk3NgAVZJn6YCKISvjcYzXJSvN68qyCOmRWpP/kmKzbbz5/xgU4JXV0E4uLX+z8USA5DObLIx7KZokQB3WSFaAwl4tkksBkzHPjwuVYw97qUBCeYn9lP4TSpVFVaIs76aMFUKvb87WoQMIQcqnQYvxWydXOXUvtxGcaqZl/z/vHu9KRBTWyXHO6TofEWC11cwFokXXelhGu64Qwf0onLA+90PLEYxRblj9C209/58vF8BNY8QCsA9d7VUHJELTneonyuF8ntU5qLGDqw34vZF1YlT5SmNHVou3vNDF7njZGd/NEz8e0izh0PowyR7mPbodHxIluT2EthaYMrPzwdr/VDw2EaK122m3Gh/Suma35p0NLgLA0gBtASYRS3S/kfecvOd4yQ1RsjGS77gDWr+/LsyQyI3P"
`endif