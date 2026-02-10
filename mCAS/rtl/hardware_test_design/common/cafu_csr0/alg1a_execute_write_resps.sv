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
`include "ccv_afu_globals.vh.iv"

module alg1a_execute_write_resps
    import ccv_afu_pkg::*;
    import cafu_common_pkg::*;
(
  input logic clk,
  input logic reset_n,    // active low reset

  /*  signals to/from the write phase FSM of the execute stage of Algorithm 1a
  */
  input logic enable_in,       // active high

  /*  signals around a SLVERR on the AXI write response channel
  */
  input logic clear_slverr,    // active high

  output logic slverr_received,
  output logic busy_out,  // active high

  /* signals for AXI-MM write responses channel
  */
  output cafu_common_pkg::t_cafu_axi4_wr_resp_ready   bready,
  input  cafu_common_pkg::t_cafu_axi4_wr_resp_ch      write_resp_chan,

  /*  signals from configuration and debug registers
  */
  input logic [8:0] NAI,
  input logic [7:0] number_of_address_increments_reg,
  input logic       single_transaction_per_set,        // active high
  input logic       force_disable_afu                   // active high
);
    
// =================================================================================================
typedef enum logic [1:0] {
  IDLE          = 'd0,
  START         = 'd1,
  CHECK_COUNT   = 'd2,
  COMPLETE      = 'd3
} fsm_enum;

fsm_enum   state;
fsm_enum   next_state;

// =================================================================================================
logic initialize;
logic pipe_1_valid;
logic pipe_2_valid;
logic pipe_2_slverr_received;
logic set_to_not_busy;

logic [8:0] pipe_3_response_count;
logic [8:0] NAI_clkd;

cafu_common_pkg::t_cafu_axi4_resp_encoding    pipe_1_resp;

// =================================================================================================
always_ff @( posedge clk )
begin
  NAI_clkd <= ~reset_n
              ? 'd0
              : force_disable_afu
                ? 'd0
                : enable_in
                  ? NAI
                  : NAI_clkd;
end

// =================================================================================================
always_ff @( posedge clk )
begin
  busy_out <= ~reset_n
              ? 1'b0
              : initialize
                ? 1'b1
                : set_to_not_busy
                  ? 1'b0
                  : busy_out;
end

// =================================================================================================
/*  this is the BREADY signal to the AXI-MM write response channel
*/
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 )                      bready <= 1'b0;
  else if( initialize == 1'b1 )                   bready <= 1'b1;
  else if( single_transaction_per_set == 1'b1 )
  begin
    if( pipe_3_response_count < 1 )               bready <= bready;
    else                                          bready <= 1'b0;
  end
  else if( pipe_3_response_count < (NAI_clkd+1) ) bready <= bready;
  else                                            bready <= 1'b0;
end

// =================================================================================================
/*  treating the BREADY signal as a 'busy' flag for this module
    if BREADY is low, valids are low
    if BREADY is high, clock the BVALID signal of the AXI-MM write response channel
*/
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 ) pipe_1_valid <= 1'b0;
  else if( bready == 1'b0 )  pipe_1_valid <= 1'b0;
  else                       pipe_1_valid <= write_resp_chan.bvalid;
end

// =================================================================================================
/*  treating the BREADY signal as a 'busy' flag for this module
    if BREADY is low, set to zero
    if BREADY is high, clock the BRESP signal of the AXI-MM write response channel
*/
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 ) pipe_1_resp <= eresp_CAFU_EXOKAY;
  else if( bready == 1'b0 )  pipe_1_resp <= eresp_CAFU_EXOKAY;
  else                       pipe_1_resp <= write_resp_chan.bresp;
end

// =================================================================================================
/*  treating the BREADY signal as a 'busy' flag for this module
    if BREADY is low, valids are low
    if BREADY is high, clock the result of the logic indicating a valid write response
*/
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 ) pipe_2_valid <= 1'b0;
  else if( bready == 1'b0 )  pipe_2_valid <= 1'b0;
  else                       pipe_2_valid <= pipe_1_valid; // want to count slverr in response count
  //else begin
  //     pipe_2_valid <= ( ( pipe_1_valid == 1'b1 )
  //                     & ( pipe_1_resp  == eresp_OKAY )
  //                     );
  //end
end

// =================================================================================================
/*  treating the BREADY signal as a 'busy' flag for this module
    if BREADY is low, set to zero
    if BREADY is high, increment by the value in pipe_2_valid (1 or 0)
*/
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 )    pipe_3_response_count <= 9'd0;
  else if( initialize == 1'b1 ) pipe_3_response_count <= 9'd0;
  else if( bready == 1'b0 )     pipe_3_response_count <= 9'd0;
  else                          pipe_3_response_count <= pipe_3_response_count + {8'd0, pipe_2_valid};
end

// =================================================================================================
/* Have to monitor bresp for a SLVERR. If received, treat like an error and record
   all errors the same except for patterns.
*/
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 )      pipe_2_slverr_received <= 1'b0;
  else if( clear_slverr == 1'b1 ) pipe_2_slverr_received <= 1'b0;
  else if( bready == 1'b0 )       pipe_2_slverr_received <= 1'b0;
  else begin
       pipe_2_slverr_received <= ( ( pipe_1_valid == 1'b1 )
                                 & ( pipe_1_resp  == eresp_CAFU_SLVERR ) )
                                 | pipe_2_slverr_received;          // once dedicated, keep it until clear
  end
end


assign slverr_received = pipe_2_slverr_received;

// =================================================================================================
always_ff @( posedge clk )
begin
       if( reset_n == 1'b0 )                state <= IDLE;
  else if( force_disable_afu== 1'b1 )       state <= COMPLETE;   // so that set_to_not_busy pulses
  else if( pipe_2_slverr_received == 1'b1 ) state <= COMPLETE;   // so that set_to_not_busy pulses
  else                                      state <= next_state;
end

// =================================================================================================
always_comb
begin
  initialize = 1'b0;
  set_to_not_busy = 1'b0;

  case( state )
    IDLE : 
    begin
      if( enable_in == 1'b1 )
      begin
                                                      next_state = START;
                                                      initialize = 1'b1;
      end
      else begin
                                                      next_state = IDLE;
      end
    end

    START :
    begin
                                                      next_state = CHECK_COUNT;

    end

    CHECK_COUNT :
    begin
           if( force_disable_afu == 1'b1 )            next_state = COMPLETE;
      else if( single_transaction_per_set == 1'b1 )
      begin
           if( pipe_3_response_count == 'd0 )         next_state = CHECK_COUNT;
           else                                       next_state = COMPLETE;
      end
      else if( pipe_3_response_count < (NAI_clkd+1) ) next_state = CHECK_COUNT;
      else                                            next_state = COMPLETE;
    end

    COMPLETE :
    begin
                                                 set_to_not_busy = 1'b1;
                                                      next_state = IDLE;
    end

    default :                                         next_state = IDLE;
  endcase
end


endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "CKUI7fxegcKVh4tgW/mpuzitVwewYkyz7TMV7+f90AhBJKvD98sjfBvbmxSG1I1A3LysG9obCcPnyzMPnYyMrcpXk+K7GRJExc8254QvDHvfaCJnqYA5D6Nac4KKUymNNkbEDeIpCIqDEhWJyac7SOjB3oz/PXRc7JbbIpxix+KArfwGhCgez+E/prMj1lSPbv0SwkqnoDiPBAW09C88cJ22OwduIBFqDrKD1asfb1KyPocquk7X/kYUKvV1RiMN0NDOBzh1Fp9KaxzAUA2XPuju3CAlhn1QlvV2LIURengVxoe/XRi1mOv5rCWZXEz7UHSu/SodPgTvWQAGzuG1fq1+Ki5DhQFgIxA/bBKVfdaoqDTAAVNIfazjBCwOxKIMPOPJrN+LZZqbBK8zAGzoiBw1sQ8BsUQfgOJPIzpY+jfquc1DIUffDBMMoiJJYyVoSLvG7XoZzdhE2C4yB+zI2I5ooiZDfpX1R7EwnEaWxOnU5l6WoTyhIvuY9dEaml2k3CMkBNKOceRZkM5PRCuIDh2P5DPG79AkAN4x12spkiLcX5FnF6N7SnA6y5Js1h1DvqGSKSdprPjAmGknWtX9vRFK0i++TjjEsdYmMhuNZ/ZsZoSSiQcyPue6UsrPGtdg/1W01ITZJyPAHF9L2pBAiA/g6pDp7uBhthDLYT2tHqs6Anu3sOpQAIsXqiYfzkJCymVlv4/MzyOiA6tyYlMS1wxqTOJ44uDllnWjP8IfIQouBYM+EtXIKDPC8yLgOB+cdgPvToQKLgYb+2kV+7j5Ndq+GH6u+XDrZoP8U64RKUH9QFsjeta0w4lKiiVdN1yGeypCUPe9x9H8v50sUC9uJY1MpVA4vumVs8WWNX4mxWJMC5CPX117p8N3PqQlGodba2vCPXLeC0yJRbQIUhs367CC9BYXsdCWL8F35l6ykLyX6pQjrhpMAOdVO93oBQxmFlT4eF2+DM5bqyEawVGImZh4ykw7yyb5Jlw29LA/zMiGZOuWsF355pRRTaG2q5w8"
`endif