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




module afu_top

import ed_cxlip_top_pkg::*;
import ed_mc_axi_if_pkg::*;
(

    input  logic                                             afu_clk,
    input  logic                                             afu_rstn,
    // April 2023 - Supporting out of order responses with AXI4
    input  ed_mc_axi_if_pkg::t_to_mc_axi4    [MC_CHANNEL-1:0] cxlip2iafu_to_mc_axi4,
    output ed_mc_axi_if_pkg::t_to_mc_axi4    [MC_CHANNEL-1:0] iafu2mc_to_mc_axi4 ,
    input  ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] mc2iafu_from_mc_axi4,
    output ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4,

    // CAS in / out
    output logic start_request,
    output logic[63:0]     target_pa,
    output logic[63:0]     compare_value,
    output logic[63:0]     swap_value,

    input logic cas_complete,
    input logic[63:0] orig_val,

    // CSR in / out
    input logic[63:0] match_ar_addr,
    input logic[63:0] match_aw_addr
);


    logic[63:0] c0_ar_addr, c1_ar_addr, c0_aw_addr, c1_aw_addr;
    logic[511:0] c0_wd_data, c1_wd_data;
    logic c0_ar_ready, c0_ar_valid, c1_ar_ready, c1_ar_valid;
    logic c0_aw_ready, c0_aw_valid, c1_aw_ready, c1_aw_valid;
    logic c0_wd_ready, c0_wd_valid, c1_wd_ready, c1_wd_valid;

    logic c0_ar_match, c0_aw_match, c1_ar_match, c1_aw_match;
    logic[5:0] c0_core_id, c1_core_id;

    ed_mc_axi_if_pkg::t_to_mc_axi4    [MC_CHANNEL-1:0] iafu2mc_to_mc_axi4_local;
    ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] mc2iafu_from_mc_axi4_local;


    assign c0_ar_addr = cxlip2iafu_to_mc_axi4[0].araddr;
    assign c0_ar_valid = cxlip2iafu_to_mc_axi4[0].arvalid;
    assign c0_ar_ready = iafu2cxlip_from_mc_axi4[0].arready;
    assign c1_ar_addr = cxlip2iafu_to_mc_axi4[1].araddr;
    assign c1_ar_valid = cxlip2iafu_to_mc_axi4[1].arvalid;
    assign c1_ar_ready = iafu2cxlip_from_mc_axi4[1].arready;

    assign c0_aw_addr = cxlip2iafu_to_mc_axi4[0].awaddr;
    assign c0_aw_valid = cxlip2iafu_to_mc_axi4[0].awvalid;
    assign c0_aw_ready = iafu2cxlip_from_mc_axi4[0].awready;
    assign c1_aw_addr = cxlip2iafu_to_mc_axi4[1].awaddr;
    assign c1_aw_valid = cxlip2iafu_to_mc_axi4[1].awvalid;
    assign c1_aw_ready = iafu2cxlip_from_mc_axi4[1].awready;

    assign c0_wd_data = cxlip2iafu_to_mc_axi4[0].wdata;
    assign c0_wd_valid = cxlip2iafu_to_mc_axi4[0].wvalid;
    assign c0_wd_ready = iafu2cxlip_from_mc_axi4[0].wready;
    assign c1_wd_data = cxlip2iafu_to_mc_axi4[1].wdata;
    assign c1_wd_valid = cxlip2iafu_to_mc_axi4[1].wvalid;
    assign c1_wd_ready = iafu2cxlip_from_mc_axi4[1].wready;

    assign c0_ar_match = (match_ar_addr != 0) & (c0_ar_addr[51:12] == match_ar_addr[51:12]) & (c0_ar_ready & c0_ar_valid);
    assign c0_aw_match = (match_aw_addr != 0) & (c0_aw_addr[51:12] == match_aw_addr[51:12]) & (c0_aw_ready & c0_aw_valid);

    assign c1_ar_match = (match_ar_addr != 0) & (c1_ar_addr[51:12] == match_ar_addr[51:12]) & (c1_ar_ready & c1_ar_valid);
    assign c1_aw_match = (match_aw_addr != 0) & (c1_aw_addr[51:12] == match_aw_addr[51:12]) & (c1_aw_ready & c1_aw_valid);

    assign c0_core_id = c0_ar_addr[11:6];
    assign c1_core_id = c1_ar_addr[11:6];


    //Passthrough User can implement the AFU logic here 
    //assign iafu2mc_to_mc_axi4      = cxlip2iafu_to_mc_axi4;
    //assign iafu2cxlip_from_mc_axi4 = mc2iafu_from_mc_axi4;

    mcas_ctrl mcas_ctrl_0 (
        .afu_clk(afu_clk),
        .afu_rstn(afu_rstn),
        .cxlip2iafu_to_mc_axi4(cxlip2iafu_to_mc_axi4),
        .iafu2mc_to_mc_axi4(iafu2mc_to_mc_axi4),
        .mc2iafu_from_mc_axi4(mc2iafu_from_mc_axi4),
        .iafu2cxlip_from_mc_axi4(iafu2cxlip_from_mc_axi4),

        .c0_ar_match(c0_ar_match),
        .c0_aw_match(c0_aw_match),

        .c0_wd_data(c0_wd_data),

        .c0_core_id(c0_core_id)
    );

endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "POizRfZBu2Za2e25gOrjvm1fIPLBk0eZmyFcDIFazcJl7PX67tT/saAlNoEXLHgw5mDQeEFh0JzMQ+qx/C0+PVE6a6spr5K6BpvxdLuS075hXOTsVE7Wc/lebFBxsNWYC7WKZkRFLi9LEIJIuDzdBuFqpnd6KNxaDlPfUh7jN8WMLzEL3yixxC+CcpZ1nL96FjsMR9I8wgkeME02AXMssvm/ZFxRfH2JRVTb/5Z7jzDsL2WpgVfQCjjHJ6iHMXgtMukpclk89l2S7mS02ZKKST94bLCO2ECwg+Qx3EKTSDKbEjLPA4iRDxcG0cx9Lm6nvljWvXWNQUxcJX5cGnR3yu0fadxCvEy/bsyJ37AQeJOTGRkhql/aCDLyCb+nZtjXCNJecS5+hX0J7UXt0aPP/5Coe4GPyIL3o13OhlUy9gnw5MMa+KXm8MoygZ9Ho+GazWtkKEhqZwR9t+9defkCmebYc0ra7/3ttH5Z4Fj7vf3vDtnGK93QnK/PLVJ3ZZqVFSvV9ddXOLiBNjNdlRglX/IE8WbqJFxGUGmUnfIm7+rfGGaHeE8STkXd+Q4OWFhGPi+7+suo1KZb0vEV45VSoWGAIkdwmMewkV6KrNqUPte75hX/Az3mhdMe/xsF8Vn/6k7CsLAxiFJrRFfEEl9JGj3aUG8PTkBg9QdhrfUBCCwIuP+ru3tHaiL7/zG3HYc2K1jnmaxgtdHxGYJ+BV/bOoO0oIUw4qSNlSQiyYaJDAgkvOeAxnlWNGol76gAwWvaVQIxlA+dD7epTUECBThTwpVRQD2b+urfoi7KmamtJ0AVQY4szoiXghLRPt/jJeIYozC/3CS6PfLYQRF0DWqLy0qV5XjFpgIbp9ciRtdgFvm5T1NZfX0hHmRqDBVahGpPCtwF7CIQ3BXw4yWF5Ib0NsPQsxpbQa65b4h5e8eSmcWGeOfQHWHWs526rgAHJtGht+TVaUDB941HR7l8hkGYKJn3qrDIABM3KG4zM5hN2PRlsGy+wpiC3cWlxctnww5f"
`endif
