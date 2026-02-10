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
`include "ccv_afu_globals.vh.iv"

module mwae_error_injection_regs
    import ccv_afu_pkg::*;
//   import ccv_afu_cfg_pkg::*;
   import tmp_cafu_csr0_cfg_pkg::*;
(
  input clk,
  input reset_n,

  `ifdef INCLUDE_POISON_INJECTION
         input [2:0] algorithm_reg,
         input       force_disable_afu,
         input       i_cache_poison_inject_busy,
  `endif

  output tmp_cafu_csr0_cfg_pkg::tmp_new_DEVICE_ERROR_INJECTION_t   new_device_error_injection_reg
);

logic cache_poison_busy;

`ifdef INCLUDE_POISON_INJECTION
  always_ff @( posedge clk )
  begin
         if( reset_n == 1'b0 )           cache_poison_busy <= 1'b0;
//    else if( algorithm_reg == 'd0 )      cache_poison_busy <= 1'b0;
    else if( force_disable_afu == 1'b1 ) cache_poison_busy <= 1'b0;
    else                                 cache_poison_busy <= i_cache_poison_inject_busy;
  end
`else
      assign cache_poison_busy = 1'b0;
`endif

assign new_device_error_injection_reg.CachePoisonInjectionBusy = cache_poison_busy;
//assign new_device_error_injection_reg.MemPoisonInjectionBusy   = 1'b0;
//assign new_device_error_injection_reg.IOPoisonInjectionBusy    = 1'b0;
//assign new_device_error_injection_reg.CacheMemCRCInjectionBusy = 1'b0;

endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHHQDb+ANhr3TiQA2xIXQwEfgF7KR0NP+yUFfjP7826sy6Y1p3+ArvAEaSpg0ri9b8PDSEUBQB6oTsbZv/HCU07gejCyuRqOnQXXoiD8sShRe4/DSvbEqrz4pVgbY8VNIkOzs4udRQriMbDMXHDkch7711MMG1tXsePRk77cy1VVwQQivNjFY/rWLa65CL5iBdD39yoOoiRJScE+V4RPX8jX39eC5BLS/xcdovTfad63KiuY49Pw96rYJRxbY1t088kI3TcxdfzpJHhFNjCBwgDdeiDvGiFpzcHA67n3OJWvw/5oHn8bbvlIudxiCZ33C/jVhM/WvPARlod3PaYEeL/Yz/bz5sZYY9R/Y+F3rVXIT6PE4twOuQZt3s+JwbhMTk0eIoy0H66/OVhpIGMBbdwp0ASVgcAh40J8/kK4xJM4SwEWnB5ZSo5787OEaZloG39EZuLjzeJU1Sh4b9KK++J3mSpCRwqb5WLBvRcFxrGL2mEFsAF+XSiw96ZQravrFixGdofWkkY/rc/rF+Yrl1zNFMbJl9ZTmmoTN0qUSPVtcJ4S/xrw6OPe2NRzF/Wn1AXfLndWVKNgHrkSGtQpqtEnsKPAd6/FcCF1qrzr8aGPiVgj6aQslpz0yO9/3LfjpMbJY5z/3usEq5oUdrVh63UNobr0ExGObR0CnvvE3y0r6/wbmGoUU2xx1AOtL1nBqUGje0p9rm8RA6nU4ICvURlHcQ5ghSCw3MDLhspCAlZBYvbbabsC8kQk5yvGCQ6xsb0+9Tnzh5/rQiS3JUqit9M+"
`endif