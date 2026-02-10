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


//------------------------------------------------------------
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
//------------------------------------------------------------

import cafu_common_pkg::*;
module intel_cxl_afu_cache_io_demux( 

    input					clk,
    input					rst,

    input					afu_cache_io_select, //afu_cache_io_select == 1 ? select io else cache
    //--to/from afu
    input   t_cafu_axi4_rd_addr_ch                   afu_axi_ar,
    output  t_cafu_axi4_rd_addr_ready                afu_axi_arready,    
    output  t_cafu_axi4_rd_resp_ch                   afu_axi_r,
    input   t_cafu_axi4_rd_resp_ready                afu_axi_rready, 
   
    input   t_cafu_axi4_wr_addr_ch                   afu_axi_aw,
    output  t_cafu_axi4_wr_addr_ready                afu_axi_awready,   
    input   t_cafu_axi4_wr_data_ch                   afu_axi_w,
    output  t_cafu_axi4_wr_data_ready                afu_axi_wready,    
    output  t_cafu_axi4_wr_resp_ch                   afu_axi_b,
    input   t_cafu_axi4_wr_resp_ready                afu_axi_bready,    

    //-- to/from cache
    output   t_cafu_axi4_rd_addr_ch                   afu_cache_axi_ar,
    input    t_cafu_axi4_rd_addr_ready                afu_cache_axi_arready,    
    input    t_cafu_axi4_rd_resp_ch                   afu_cache_axi_r,
    output   t_cafu_axi4_rd_resp_ready                afu_cache_axi_rready, 
   
    output   t_cafu_axi4_wr_addr_ch                   afu_cache_axi_aw,
    input    t_cafu_axi4_wr_addr_ready                afu_cache_axi_awready,   
    output   t_cafu_axi4_wr_data_ch                   afu_cache_axi_w,
    input    t_cafu_axi4_wr_data_ready                afu_cache_axi_wready,    
    input    t_cafu_axi4_wr_resp_ch                   afu_cache_axi_b,
    output   t_cafu_axi4_wr_resp_ready                afu_cache_axi_bready,    

    //-- to/from io
    output   t_cafu_axi4_rd_addr_ch                   afu_io_axi_ar,
    input    t_cafu_axi4_rd_addr_ready                afu_io_axi_arready,    
    input    t_cafu_axi4_rd_resp_ch                   afu_io_axi_r,
    output   t_cafu_axi4_rd_resp_ready                afu_io_axi_rready, 
   
    output   t_cafu_axi4_wr_addr_ch                   afu_io_axi_aw,
    input    t_cafu_axi4_wr_addr_ready                afu_io_axi_awready,   
    output   t_cafu_axi4_wr_data_ch                   afu_io_axi_w,
    input    t_cafu_axi4_wr_data_ready                afu_io_axi_wready,    
    input    t_cafu_axi4_wr_resp_ch                   afu_io_axi_b,
    output   t_cafu_axi4_wr_resp_ready                afu_io_axi_bready

);

    assign afu_io_axi_ar         = afu_cache_io_select ? afu_axi_ar             : 'h0  ;
    assign afu_io_axi_rready     = afu_cache_io_select ? afu_axi_rready         : 'h0  ;
    assign afu_io_axi_aw         = afu_cache_io_select ? afu_axi_aw             : 'h0  ;
    assign afu_io_axi_w          = afu_cache_io_select ? afu_axi_w              : 'h0  ;
    assign afu_io_axi_bready     = afu_cache_io_select ? afu_axi_bready         : 'h0  ;

    assign afu_axi_arready       = afu_cache_io_select ? afu_io_axi_arready     :  afu_cache_axi_arready  ;
    assign afu_axi_r             = afu_cache_io_select ? afu_io_axi_r           :  afu_cache_axi_r        ;
    assign afu_axi_awready       = afu_cache_io_select ? afu_io_axi_awready     :  afu_cache_axi_awready  ;
    assign afu_axi_wready        = afu_cache_io_select ? afu_io_axi_wready      :  afu_cache_axi_wready   ;
    assign afu_axi_b             = afu_cache_io_select ? afu_io_axi_b           :  afu_cache_axi_b        ;

    assign afu_cache_axi_ar      = afu_cache_io_select ? 'h0 : afu_axi_ar       ;
    assign afu_cache_axi_rready  = afu_cache_io_select ? 'h0 : afu_axi_rready   ;
    assign afu_cache_axi_aw      = afu_cache_io_select ? 'h0 : afu_axi_aw       ;
    assign afu_cache_axi_w       = afu_cache_io_select ? 'h0 : afu_axi_w        ;
    assign afu_cache_axi_bready  = afu_cache_io_select ? 'h0 : afu_axi_bready   ;

endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "POizRfZBu2Za2e25gOrjvm1fIPLBk0eZmyFcDIFazcJl7PX67tT/saAlNoEXLHgw5mDQeEFh0JzMQ+qx/C0+PVE6a6spr5K6BpvxdLuS075hXOTsVE7Wc/lebFBxsNWYC7WKZkRFLi9LEIJIuDzdBuFqpnd6KNxaDlPfUh7jN8WMLzEL3yixxC+CcpZ1nL96FjsMR9I8wgkeME02AXMssvm/ZFxRfH2JRVTb/5Z7jzCiz0mHR5W+wq7RZ2lF4Cq8TVKarkwY+dOR0VM59SybRRa74kmDZUGFeDFnGaGcMPS1BH98p2NTk2z6WeVmCHZAnRivzh2VZx2pCuYESpeSH2n9Ic/thwVuiAsBf+0IwqwHm2YLWSVweK6Wk3MB0kDQnmrE5iSbZH+GwcIQliF4cFsUze5s01Ld6tTJVpXYb/cFNLusKp8Sqmi/cNvlL7QYlSQiOEqUhxoebzAfn3fKu9Us440dU+xmfXLV4uS2pxgGQuhG0K7UEgMiY1uFOrrH4cnDspGAEzgJTu28fLuDi4otWOJHBHBcJ8OYYrmPw0lT1hPoi/mQEWseaYyagca0P3NuM2ovwYP029e3AJKueBE8+uYY762GO83nurxH4AVWRzzMywapqguLEHurKfVEv6LF2LJ+yAAsRKSljQ1vGBm3LK1z77w1A1up3bfPT8qJ86yXbgIWi4P4pwB4BkUTEJmKCJEDEtOyltfVyPvHrtj0UlfF5C+cIuQ1MOTP40SX9dbw0ZwyZ+cglhdCSrXytcrHfIcZM+6/SoLlvKBxKgvstTa0pW8v8nvQ84AvztaGUB1ntuegLNUGSn4iA4MOR5LGnOTwL2SpYOR5i6VYkHbawS9L/e8Zzf88iUlP2Q9iFVa5a4Rw3baA1nuZoBXPwrFpytGrRcLsnxBs4VBfJPt5g57TtPE8069A4vfeiixhlnqjkPZJ8w2j0L9YATze6E4HmN+QFJtkW/nrI77LePgJHOQCYZCsYo7RF9fpljt4+uAL+1NX9YT3AOJ7PsZM"
`endif