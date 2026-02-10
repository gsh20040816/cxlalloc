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


// Copyright 2024 Intel Corporation.
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
/*
  Description   : FPGA CXL Compliance Engine Initiator AFU
                  Speaks to the AXI-to-CCIP+ translator.
                  This afu is the initiatior
                  The axi-to-ccip+ is the responder
*/

`ifndef CCV_AFU_PKG_VH
`define CCV_AFU_PKG_VH

package ccv_afu_pkg;

//-------------------------
//------ Parameters
//-------------------------
localparam CCV_AFU_DATA_WIDTH   =   512;
localparam CCV_AFU_ADDR_WIDTH   =   52;


typedef struct packed {
  logic illegal_base_address;
  logic illegal_protocol_value;
  logic illegal_write_semantics_value;
  logic illegal_read_semantics_execute_value;
  logic illegal_read_semantics_verify_value;
  logic illegal_pattern_size_value;
} config_check_t;


//-------------------------
//------ Parameters for Alg7
//-------------------------
typedef struct packed {
  logic [8:0]   axi_id;
  logic [51:0]  real_address;
  logic [511:0] extended_real_pattern;
} alg7_fifo_data_t;

localparam ALG7_FIFO_DATA_BW = $bits(alg7_fifo_data_t);
localparam ALG7_FIFO_DEPTH = 16;
localparam ALG7_FIFO_PTR_BW = $clog2(ALG7_FIFO_DEPTH);








endpackage: ccv_afu_pkg

`endif  // `define CCV_AFU_PKG_VH
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHElvme7bvzGgC7iveRGPb2Da1+5e669vtshlE0fkoM0n5ilym4oxWM8ibR8tZQOoEqoaa3V6TfezY1dxgzZJfXd5lyXJC1NK7p3OIc8s8g5yTBPWTlMOkRnrL1PzKwAKLtQN4+w/Y3alQfBYjMcur/rZ9MC7V82R/ZSEcROIyZZXcskm8sPmQz4nFP1JO/Xqmazk9WYq6B7Y+wD+YRm4qWokH+sSigolMkJrkkdtAhgcjLFbRp9kpwrpzchXVgLTPKh6kB9s+3BcA/cb5cbbbsmOfKN989XzED2TLU388Jech7JZJn6M3otQg4CforKjjaxdYJPC+vNukOBSm3b8g59cxc+gc12i99S/bScvV6vljwE5VChFKJK+2nCVw5rfHfyXeNJkR6qdcPAWzWBZSNweyx16stvz9Qx5svuUrKO94ZWia51peXMkdERkwAOja7O2sAJ5yRFl0wYr8xTuDsskHR775iys70uePgUG3/77fEzEpD9hGu3VTf9yvU+aqYVjGmYSbH1Iu5sgsCeFjWPU139AKhhdTo23UymFZ7LSjsSh4gzPoR/HL0zYiw7pSR+pfIDPqSCJrSkAxzoQ1u9F7gdz9AGvhI9j/a9+EAmLIQiu51Jc03dCXYM3DQ5isVSh8D1rZxm7e0437aLAHszvERJPAd9kakslVrFAuI8NxatceG9ycb87q0Vm0DT2Ar4bgIZp7M/qvxwZAruxiFJ6FPomXpV8QmbPgSnwmDw8Xbw3ugj4pdcazkJhwKoGTobhFQdYhUV2R2ZoSYD2HVs"
`endif