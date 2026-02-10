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

module mwae_afu_status_regs
//   import ccv_afu_cfg_pkg::*;
   import tmp_cafu_csr0_cfg_pkg::*;
   import ccv_afu_pkg::*;
(
  input clk,
  input reset_n,                //  active low
  input i_mwea_top_level_fsm_busy,
  input i_alg_1a_execute_busy,
  input i_alg_1a_verify_sc_busy,
  input i_alg_1a_verify_nsc_busy,
  input i_alg_1b_execute_busy,
  input i_alg_1b_verify_sc_busy,
  input i_alg_1b_verify_nsc_busy,
  input i_alg_2_execute_busy,

  input [7:0]  i_current_loop_number,
  input [3:0]  i_current_set_number,
  input [31:0] i_current_base_pattern,
  input [51:0] i_current_base_address,

  output tmp_cafu_csr0_cfg_pkg::tmp_new_DEVICE_AFU_STATUS1_t device_afu_status_1_reg,
  output tmp_cafu_csr0_cfg_pkg::tmp_new_DEVICE_AFU_STATUS2_t device_afu_status_2_reg
);


always_ff @( posedge clk )
begin
  if( reset_n == 1'b0 ) 
  begin
       device_afu_status_1_reg.afu_busy               <= 1'b0;
       device_afu_status_1_reg.alg_execute_busy       <= 1'b0;
       device_afu_status_1_reg.alg_verify_nsc_busy    <= 1'b0;
       device_afu_status_1_reg.alg_verify_sc_busy     <= 1'b0;
       device_afu_status_1_reg.loop_number            <= 'd0;
       device_afu_status_1_reg.set_number             <= 'd0;
       device_afu_status_1_reg.current_base_pattern   <= 'd0;
  end
  else begin
       device_afu_status_1_reg.afu_busy               <= i_mwea_top_level_fsm_busy;
       device_afu_status_1_reg.alg_execute_busy       <= i_alg_1a_execute_busy;
       device_afu_status_1_reg.alg_verify_nsc_busy    <= i_alg_1a_verify_nsc_busy;
       device_afu_status_1_reg.alg_verify_sc_busy     <= i_alg_1a_verify_sc_busy;
       device_afu_status_1_reg.loop_number            <= i_current_loop_number;
       device_afu_status_1_reg.set_number             <= i_current_set_number;
       device_afu_status_1_reg.current_base_pattern   <= i_current_base_pattern;
  end
end

always_ff @( posedge clk )
begin
  if( reset_n == 1'b0 )  device_afu_status_2_reg.current_base_address <= 'd0;
  else                   device_afu_status_2_reg.current_base_address <= i_current_base_address;
end


endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHF0A+cUmVFPga45E5Ru6s9dRT4ZHMOfPZ//6zzmMGM2PS/opG6NpHitSuLqwSZKWTsscQUqnXdJRobkoprM0DXvGutDUtz6e9Zgt8gGWJi8B4q62BpxhGxjgvrO0KaJ/DIOn/7CZRojguO76JWfeHQMAVoS5y5yww3X5ppL2OMHUD1WG2f4jmDwqbLz6KQokIaEAbuUfdwvrAHqnACiWo9+9c7d73n59yuQMjNY69pT/wg/ZHqcDNIkXiRW1aVPwwX9nTHX5KCoXqXmhS9T4JvV3s0g4QddNrItiRKHVXUY4L5BBBtb4lVtFBd0atgF+WrXHUAwtiMTd8WG8yEwJYhJTmfwzPzwOMi6huK8Z7rVGGa3c1ZMXUYqiEynQhyVLo98OSZBW2Oa/iZuCZL7GT2IFlt/fPv51VRfXZGuzXu8Kf/bQq55JJ3/ziBQS6674BpWNEzWM+IKFPMrBVDeSid6H95inNv+p63a27C0LzzLhxmJFIm58e9BG4wdAN9BLooVLVfoc5FfAq+bAHMjiyBfAt+nCGVkV0iloQ9dn/T6t7dVJLVE/voMucsKNrXCbL8MtBhhtw63nf3sErpI+fK8q9x/yb2KH4D/ds9h6uUngTbmWbDG8g8Fe6EO7xCYFJhC9vYrzfMIEJOlihTenYoTlOMMZdHPFhuuE4rvRROOVRYx3zjQryCC/fHd4Y/rhRBx1q1vWsJ1Mdx3m1kRXRwXf/J63xPCsTxag/rPLygEyMkxyq8Dx68+CZaNHy7yXjWQ/te2vtpD1JGYbuZuA+4I"
`endif