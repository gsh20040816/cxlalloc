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
/*
  Description   : FPGA CXL Compliance Engine Initiator AFU
                  Speaks to the AXI-to-CCIP+ translator.
                  This afu is the initiatior
                  The axi-to-ccip+ is the responder
*/

`ifndef CCV_AFU_ALG1A_PKG_VH
`define CCV_AFU_ALG1A_PKG_VH

package ccv_afu_alg1a_pkg;

//-------------------------
//------ Parameters
//-------------------------

typedef enum logic [1:0] {
  MODE_IDLE        = 2'd0,
  MODE_EXECUTE     = 2'd1,
  MODE_VERIFY_SC   = 2'd2,
  MODE_VERIFY_NSC  = 2'd3
} alg1a_mode_enum;

typedef enum logic [3:0] {
  AXI_WR_IDLE            = 4'd0,
//  AXI_WR_WAIT_TIL_4      = 4'd1,
  AXI_WR_WAIT_TIL_NOT_EMPTY = 4'd1,
  AXI_WR_FIRST_POP       = 4'd2,
  AXI_WR_FIRST_AWVALID   = 4'd3,
  AXI_WR_FIRST_AWREADY   = 4'd4,
  AXI_WR_NEXT_AWREADY    = 4'd5,
  AXI_WR_NEXT_AWVALID    = 4'd6,
  AXI_WR_LAST_AWREADY    = 4'd7,
  AXI_WR_LAST_AWVALID    = 4'd8,
  AXI_WR_LAST_WREADY     = 4'd9,
  AXI_WR_LAST_WVALID     = 4'd10,

  AXI_WR_FIRST_WAIT              = 4'd11,
  AXI_WR_FIRST_AWREADY_PLUS_POP2 = 4'd12



} alg1a_exe_axi_write_fsm_enum;







endpackage: ccv_afu_alg1a_pkg

`endif
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHFPK50361SJkMzZfpnb5FybltlJrTDzMz0XNOMIx4dkQpGavKwsNM5Rs9P/oqVIYQi3C4sIPrFI7l8TXGO5WQvRGwL/h5Y5c/I4yz8yHYMw4dhZglQOM8Nk9rPQDwK93t8FqWg/Rhu2MfWwHjSlHflko+uzwkN1t7XIfvwSecIoC/FUT4UKNfStQiiBsrHcnMTDjXcg2oBcXFjRCRKGeXO4xK9QHtC4vujL73+VEpusXZ2zRd/fJucQxS7l3rGCBQqbQMnTPYxEA+D1HYRPcnNXf5pHkzJxJXL+P5TQxrJZmTS5jJSxt4qSVTa9FZVy5fowuyCYU47VIa+ac5FvriT0w5QrQ8+zrfrm+Dots2NziyIi4DfxVX8a5WPzJTwSnFBRI7et4rKWD0i5FrjUSh7XemcpM1dp2jBg/SXF2BTQ+KHeHxyUQkeO4YE2/N9OwVydkeYqpzJM7k2vB9kzd0WF6yAElq5OnAuhUJCl/EXSpIuZZbbUfQki/sB7G5QsIhNMtMy3u/+LBRYGAL4kSUJvfjFATdQuKeoHILleqJZalWYEG/3MzVpZW4Ut78S6xiInPKzzGfvUQRNXrnmAHie0Lct2/HsQ2wG2UdSuuWa7G0FlDqggMnvOvCTciLR9EBabBuV044A0Ecl4No6Rldu331xGh5EG1tsG8+knGSMtGN4qlXz+LCnaoFzlHTaoN1HaFVLkuSwXFH3w3BQ98YKt+jOMvZTOzXGZ+OFBFLCKRWnkZnX7cS2n98sdBHKfCXmYZBO2/Hht0YGSArEFVEJQ"
`endif