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

module ex_default_csr_top (
    input  logic        csr_avmm_clk,
    input  logic        csr_avmm_rstn,  
    output logic        csr_avmm_waitrequest,  
    output logic [63:0] csr_avmm_readdata,
    output logic        csr_avmm_readdatavalid,
    input  logic [63:0] csr_avmm_writedata,
    input  logic        csr_avmm_poison,
    input  logic [21:0] csr_avmm_address,
    input  logic        csr_avmm_write,
    input  logic        csr_avmm_read, 
    input  logic [7:0]  csr_avmm_byteenable,

    input  logic       afu_clk,
    input  logic [1:0] cxlip2iafu_read_eclk,
    input  logic [1:0] cxlip2iafu_write_eclk,

    output logic [63:0]    csr_op_pa, // byte level address
    output logic [511:0]    csr_op_wr_data, 
    output logic           csr_start_op,
    input  logic           csr_end_op,
    input  logic [63:0]   csr_op_ret_data,
    output logic [63:0]    csr_config_bits,
    input  logic [63:0]    csr_timer,

    input logic start_request,
    input logic[63:0]     target_pa,
    input logic[63:0]     compare_value,
    input logic[63:0]     swap_value,

    output logic[63:0]     match_ar_addr,
    output logic[63:0]     match_aw_addr
);

//CSR block

   ex_default_csr_avmm_slave ex_default_csr_avmm_slave_inst(
       .clk          (csr_avmm_clk),
       .reset_n      (csr_avmm_rstn),
       .writedata    (csr_avmm_writedata),
       .read         (csr_avmm_read),
       .write        (csr_avmm_write),
       .poison       (csr_avmm_poison),
       .byteenable   (csr_avmm_byteenable),
       .readdata     (csr_avmm_readdata),
       .readdatavalid(csr_avmm_readdatavalid),
       .address      ({10'h0,csr_avmm_address}),
       .waitrequest  (csr_avmm_waitrequest),

       .afu_clk                    (afu_clk),
       .cxlip2iafu_read_eclk       (cxlip2iafu_read_eclk),
       .cxlip2iafu_write_eclk      (cxlip2iafu_write_eclk),

       .csr_op_pa       (csr_op_pa), 
       .csr_op_wr_data  (csr_op_wr_data),
       .csr_start_op    (csr_start_op),
       .csr_end_op      (csr_end_op),
       .csr_op_ret_data (csr_op_ret_data),
       .csr_config_bits (csr_config_bits),
       .csr_timer       (csr_timer),

       .start_request(start_request),
       .target_pa(target_pa),
       .compare_value(compare_value),
       .swap_value(swap_value),

       .match_ar_addr(match_ar_addr),
       .match_aw_addr(match_aw_addr)
   );

//USER LOGIC Implementation 
//
//


endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwDCBv60Tz54ilg4WPqbBwdkOTyzAgNO7Yti/LVuGGyyMwRXet/VBhiJeBdNRVABvJul5OdXRWIacJ8TV7TroFZF8iL7bdy6YM6txryBoK2n+UODDs1dzHkag1WcjmAGbDR8yo9OtHEplmAy+5SX08Xv3KSLRuYXMp1V/EkPZhzzn3GIaDmY3DM5bNUasMPdBMY0O6X00NkTO2s2qX+OycAqxGT5WvDvyRVjRxjUJn9tlNoMz2f6dn1KSrb5z3z43u4bY982lzlmen7MtQZYeHPON7qtUElzkZgK20R46mRYuUWlyCfF9qn/JngneEw0gEAuQ+6aNGIQHr2x9ALPFTB6iGB0oEjrmqCNsFxaCZ1C1zUpteCmDl+UTrgy0RbspCCAKA7rj9XWONy+yuVHcKQPla4kxP7H1W1gJzubQ0y7xQbSjS/MxPcrYUrWqaFHCI04ITangD7cHKkGOf5o9uY17rebSvLSl46hLQw997bA4BIzYBRhMFN7BS474qkGB6tYRxuuz5GaHsj1cM4Vfqlt3s0Eg/VjiGWKtEyuM5coaHFToWJoFcKHdWqT8sRH24RRCnwdwvE9NHGvDT635QuUCf02hc44q+Klj14QDQa2q7uYJ5YKjouIiZCaTX2PQ2qT3N5QtGgjxzhT1/krbSXcWfJXWBy+1nTzKluZkADo3VCi/QeuEs1fPzAdz96QDoI4kqLa/lHa/YPXIecgCZ94OLlA+XEyyor+MWynKWpKYdU40k+pM/pfvY7Z3RnufOu2kbvVCnGvChfktjeIqgeo"
`endif
