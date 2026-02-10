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
/*                COHERENCE-COMPLIANCE VALIDATION AFU

  Description   : FPGA CXL Compliance Engine Initiator AFU
                  Speaks to the AXI-to-CCIP+ translator.
                  This afu is the initiatior
                  The axi-to-ccip+ is the responder

  initial -> 07/12/2022 -> Antony Mathew
*/


module cust_afu_wrapper
(
      // Clocks
  input logic  axi4_mm_clk, 

    // Resets
  input logic  axi4_mm_rst_n,

  /*
    AXI-MM interface - write address channel
  */
  output logic [11:0]               awid,
  output logic [63:0]               awaddr, 
  output logic [9:0]                awlen,
  output logic [2:0]                awsize,
  output logic [1:0]                awburst,
  output logic [2:0]                awprot,
  output logic [3:0]                awqos,
  output logic [5:0]                awuser,
  output logic                      awvalid,
  output logic [3:0]                awcache,
  output logic [1:0]                awlock,
  output logic [3:0]                awregion,
  output logic [5:0]                awatop,
   input                            awready,
  
  /*
    AXI-MM interface - write data channel
  */
  output logic [511:0]              wdata,
  output logic [(512/8)-1:0]        wstrb,
  output logic                      wlast,
  output logic                      wuser,
  output logic                      wvalid,
 // output logic [7:0]                wid,
   input                            wready,
  
  /*
    AXI-MM interface - write response channel
  */ 
   input [11:0]                     bid,
   input [1:0]                      bresp,
   input [3:0]                      buser,
   input                            bvalid,
  output logic                      bready,
  
  /*
    AXI-MM interface - read address channel
  */
  output logic [11:0]               arid,
  output logic [63:0]               araddr,
  output logic [9:0]                arlen,
  output logic [2:0]                arsize,
  output logic [1:0]                arburst,
  output logic [2:0]                arprot,
  output logic [3:0]                arqos,
  output logic [4:0]                aruser,
  output logic                      arvalid,
  output logic [3:0]                arcache,
  output logic [1:0]                arlock,
  output logic [3:0]                arregion,
   input                            arready,

  /*
    AXI-MM interface - read response channel
  */ 
   input [11:0]                     rid,
   input [511:0]                    rdata,
   input [1:0]                      rresp,
   input                            rlast,
   input                            ruser,
   input                            rvalid,
   output logic                     rready
  

   
);

// Tied to Zero for all inputs. USER Can Modify

//assign awready = 1'b0;
//assign wready  = 1'b0;
//assign arready = 1'b0;
//assign bid     = 16'h0;
//assign bresp   = 4'h0;  
//assign buser   = 4'h0;
//assign bvalid  = 1'b0;
//
//assign rid     = 16'h0; 
//assign rdata   = 512'h0;
//assign rresp   = 4'h0;
//assign rlast   = 1'b0;
//assign ruser   = 4'h0;
//assign rvalid  = 1'b0;


  assign  awid         = '0   ;
  assign  awaddr       = '0   ; 
  assign  awlen        = '0   ;
  assign  awsize       = '0   ;
  assign  awburst      = '0   ;
  assign  awprot       = '0   ;
  assign  awqos        = '0   ;
  assign  awuser       = '0   ;
  assign  awvalid      = '0   ;
  assign  awcache      = '0   ;
  assign  awlock       = '0   ;
  assign  awregion     = '0   ;
  assign  awatop       = '0   ;
  assign  wdata        = '0   ;
  assign  wstrb        = '0   ;
  assign  wlast        = '0   ;
  assign  wuser        = '0   ;
  assign  wvalid       = '0   ;
//  assign  wid          = '0   ;
  assign  bready       = '0   ;
  assign  arid         = '0   ;
  assign  araddr       = '0   ;
  assign  arlen        = '0   ;
  assign  arsize       = '0   ;
  assign  arburst      = '0   ;
  assign  arprot       = '0   ;
  assign  arqos        = '0   ;
  assign  aruser       = '0   ;
  assign  arvalid      = '0   ;
  assign  arcache      = '0   ;
  assign  arlock       = '0   ;
  assign  arregion     = '0   ;
  assign  rready       = '0   ;


endmodule


`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwC/COVkHU40sJ5GuE1N0VE9aSSfgrrrGktfkSpPjVt+6vh4HQWnJKN/Lzk9oLSt1YLysf4dnGsynk5SDuZ6x0/ML5ubbSJVc2tgfUQwmLerlUi4bx1nUsqrq/XH3R3dANFxbqzbmpKGXMvqgb46n9Sm37rY4V/TYZFd4PzsLiyNDHYKdmnJvewxrFm/uLgERXzkW7lcD8KAcvUUAaz4Z4C2zY5+qyc9GNCb7RltAE5tYC7liWkMk4z8BnG2dlufPlefmUQxzKaSCbT//Vli6sDXBFM3D9suAASp02oo2BhDWdvFyy4pq0iWjukOdB3EikBjEhsO21PYCzWzqbJIdlJFSDW6SRUfFXdGUxxEFI+6M1p5Xv62vo3vJ8NGZFEbvOvExF+4yuN0UKpuJHAGlNGe+PmM5t8B7Zg42sjnL6BO9coYVmwaZpVMsVG4a6Bih2wyQyUDM+Cd8emewQyVnoM+mgxnFNl6CQj95XQajtj1yByTKjuKxokeGiPq3NCQ3hgOWESoSoYZwtb+gE86VbYMp8zzZY5D48aITm6wffPYQOy8ptHbOOYX2cTrFQtnKNtC9aTyiZcsumTRcqRrY9wXfEIEawlIhNSxaZccRwMmgp0LlsIrB/C7bfyOUM0KhlX08n+4+cYFeqB2nJS12LfMBaiOVRDLMpND1lHDDJpNtAd3by3ADzjZ563qTDTG1aJ14tfktADcuFUKjTsy9ZM74qHlAxxESSLyJORVAqqQ5533UJBV6W0BVXBwKWPJbT2NineVOGPZKKx4ewKvcIUu"
`endif