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
module intel_cxl_afu_pio_mux (
	
    input				    clk,
    input				    rstn,
    input   logic  [0:0]    afu_pio_select,         //afu_pio_select==1  ?  select  afu  else  pio
    input   logic  [0:0]    pio_tx_st_eop,                                                     
    input   logic  [127:0]  pio_tx_st_header,                                                  
    input   logic  [255:0]  pio_tx_st_payload,                                                 
    input   logic  [0:0]    pio_tx_st_sop,                                                     
    input   logic  [0:0]    pio_tx_st_hvalid,                                                  
    input   logic  [0:0]    pio_tx_st_dvalid,                                                  
    input   logic  [0:0]    afu_tx_st_eop,                                                     
    input   logic  [127:0]  afu_tx_st_header,                                                  
    input   logic  [512:0]  afu_tx_st_payload,                                                 
    input   logic  [0:0]    afu_tx_st_sop,                                                     
    input   logic  [0:0]    afu_tx_st_hvalid,                                                  
    input   logic  [0:0]    afu_tx_st_dvalid,                                                  
    output  logic  [0:0]    avst_tx_st0_eop_o,                                                 
    output  logic  [127:0]  avst_tx_st0_header_o,                                              
    output  logic  [255:0]  avst_tx_st0_payload_o,                                             
    output  logic  [0:0]    avst_tx_st0_sop_o,                                                 
    output  logic  [0:0]    avst_tx_st0_hvalid_o,                                              
    output  logic  [0:0]    avst_tx_st0_dvalid_o,                                              
    output  logic  [0:0]    avst_tx_st0_ready_i,                                               
    output  logic  [0:0]    avst_tx_st1_eop_o,                                                 
    output  logic  [127:0]  avst_tx_st1_header_o,                                              
    output  logic  [255:0]  avst_tx_st1_payload_o,                                             
    output  logic  [0:0]    avst_tx_st1_sop_o,                                                 
    output  logic  [0:0]    avst_tx_st1_hvalid_o,                                              
    output  logic  [0:0]    avst_tx_st1_dvalid_o                                               
);


always_ff@(posedge clk) begin

	if(afu_pio_select) begin
		avst_tx_st0_eop_o   	<= 1'b0;
		avst_tx_st0_header_o	<= afu_tx_st_header;
		avst_tx_st0_payload_o	<= afu_tx_st_payload[255:0];
		avst_tx_st0_sop_o   	<= afu_tx_st_sop;
		avst_tx_st0_hvalid_o	<= afu_tx_st_hvalid;
		avst_tx_st0_dvalid_o	<= afu_tx_st_dvalid; 
		avst_tx_st1_eop_o   	<= afu_tx_st_eop; 
		avst_tx_st1_header_o	<= 128'h0; 
		avst_tx_st1_payload_o	<= afu_tx_st_payload[511:256]; 
		avst_tx_st1_sop_o   	<= 1'h0; 
		avst_tx_st1_hvalid_o	<= 1'h0; 
		avst_tx_st1_dvalid_o	<= afu_tx_st_dvalid; 
	end
	else begin
		avst_tx_st0_eop_o   	<= pio_tx_st_eop;
		avst_tx_st0_header_o	<= pio_tx_st_header;
		avst_tx_st0_payload_o	<= pio_tx_st_payload;
		avst_tx_st0_sop_o   	<= pio_tx_st_sop;
		avst_tx_st0_hvalid_o	<= pio_tx_st_hvalid;
		avst_tx_st0_dvalid_o	<= pio_tx_st_dvalid; 
		avst_tx_st1_eop_o   	<= 1'h0; 
		avst_tx_st1_header_o	<= 128'h0; 
		avst_tx_st1_payload_o	<= 256'h0; 
		avst_tx_st1_sop_o   	<= 1'h0; 
		avst_tx_st1_hvalid_o	<= 1'h0; 
		avst_tx_st1_dvalid_o	<= 1'h0; 
	end
end //always


endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "2M3FFSmZRo+OLDXkYi9QgRKIMy2+kBf8+s3s4c/izvMHLMxClhU2h++VjBIj3jsb5ZYIvLYWFsScMuG6+Wc4dsRtKcSj6DjH0dACKmVLAW04huWOolapjZ+Qsree1nouVGRaWBxKtEz7Pc+upAJSBmn/6rKL+D4mnksigk9L8DhC5yXuN7JKuDl1odnnzvJcc/Z8zZ3EENdv8TME2Fpb1a4B0w6wIbaMCHaK7JxR2QWoorqyxuIX2yJSI3mVCA2EJ2dFK+zlM8Pk+BkmPHYU0wm4eCxjfSRqO5FxJu4Y85Q9oi6CkokRd82dU+Ao6xVBXmCO6JbAqwboWp+GwcYl8ZCe4PivjsRhedA1wnxgIgGTKbj9cpZIwODFMHPa5VM0pxm8L4+gIdy1UhkYmS9B79QdmGZN/o14zIqu46nJr4j70H4AZezCKcfZaS/8vrBYmJhwIZDv6j06dSNSW5zrJwj8h4vKoDIEGMCzFhkerdTWOxJ3oopeChNUoawr7a0mq1VyQVuRJDLrKZvWMFs28oWvZb3Hui/BOqh7puWlkxvWNT5BMgZUe5xiQhu5wjcLxe1jTCUk5xvbUMzvJxWjq6YfEvL6swYoC1GBRhiqJGXAOo9GPVSdkHtM1rzn/flJQNEEIKNC7QZErcpWKHYWIFA5OXu/xVElT7K/srDN/Xcep1NL9aPHbt1pVHS3RflxXNcYEs7OURwA86CTCWXniWNEVvK5XTqsGfMnXe7OLk0bmT2RH+co5fJ4KvaJyBFpMzkeNk+rzQuMlyFGQURgYQVsNAi+d39FosK8xpv2BXco522axCuQ2uxALV92dGieaHdT/LBybbCyg2/MEUDfcaj2ci7fc2P4CXDx0jzO4n1PfFC3CVsJluGLFAfDtRQtbStVDzkqKabtMchFibcin/90gac7dQ+Shk5AEfUopKUp8UikZe6w/pcSOqimU2rnrn2+mtq24Cs1sBWuFOJJGIkdaQjhf5jKgOTYGyHNquxN0lE11jg/1vNsS44drljp"
`endif