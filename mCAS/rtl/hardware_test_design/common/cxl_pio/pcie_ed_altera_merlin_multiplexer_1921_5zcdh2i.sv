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


// (C) 2001-2023 Intel Corporation. All rights reserved.
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


// (C) 2001-2014 Altera Corporation. All rights reserved.
// Your use of Altera Corporation's design tools, logic functions and other 
// software and tools, and its AMPP partner logic functions, and any output 
// files any of the foregoing (including device programming or simulation 
// files), and any associated documentation or information are expressly subject 
// to the terms and conditions of the Altera Program License Subscription 
// Agreement, Altera MegaCore Function License Agreement, or other applicable 
// license agreement, including, without limitation, that your use is for the 
// sole purpose of programming logic devices manufactured by Altera and sold by 
// Altera or its authorized distributors.  Please refer to the applicable 
// agreement for further details.



// ------------------------------------------
// Merlin Multiplexer
// ------------------------------------------

// altera message_off 13448

`timescale 1 ns / 1 ns


// ------------------------------------------
// Generation parameters:
//   output_name:         pcie_ed_altera_merlin_multiplexer_1921_5zcdh2i
//   NUM_INPUTS:          1
//   ARBITRATION_SHARES:  1
//   ARBITRATION_SCHEME   "no-arb"
//   PIPELINE_ARB:        0
//   PKT_TRANS_LOCK:      1220 (arbitration locking enabled)
//   ST_DATA_W:           1267
//   ST_CHANNEL_W:        1
// ------------------------------------------

module pcie_ed_altera_merlin_multiplexer_1921_5zcdh2i
(
    // ----------------------
    // Sinks
    // ----------------------
    input                       sink0_valid,
    input [1267-1   : 0]  sink0_data,
    input [1-1: 0]  sink0_channel,
    input                       sink0_startofpacket,
    input                       sink0_endofpacket,
    output                      sink0_ready,


    // ----------------------
    // Source
    // ----------------------
    output reg                  src_valid,
    output [1267-1    : 0] src_data,
    output [1-1 : 0] src_channel,
    output                      src_startofpacket,
    output                      src_endofpacket,
    input                       src_ready,

    // ----------------------
    // Clock & Reset
    // ----------------------
    input clk,
    input reset
);
    localparam PAYLOAD_W        = 1267 + 1 + 2;
    localparam NUM_INPUTS       = 1;
    localparam SHARE_COUNTER_W  = 1;
    localparam PIPELINE_ARB     = 0;
    localparam ST_DATA_W        = 1267;
    localparam ST_CHANNEL_W     = 1;
    localparam PKT_TRANS_LOCK   = 1220;
    localparam SYNC_RESET       = 1;

    assign	src_valid			=  sink0_valid;
    assign	src_data			=  sink0_data;
    assign	src_channel			=  sink0_channel;
    assign	src_startofpacket  	        =  sink0_startofpacket;
    assign	src_endofpacket		        =  sink0_endofpacket;
    assign	sink0_ready			=  src_ready;
endmodule


`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwDSq/wk1bAjkFFIu091r1V+OMhczpbxUfvOmRQNYPwTL7HOnUZuD4RHHWCESsgXxAYksQmyXhWDUIOFTuXoSq7hA8i6DqRBsVz2a6+rzUWzT85QzZUFdMJ+uXdZDumnVG1MJQCBtj5NFw6gsQhTJw3AhY8cGTOZReRmPUrXYUgdT/nP3j/OmIa6dxdZW8nhd+t/UxfdVFFDQDTSegGNRjWEieyC7rRnTXpvCb3ZBLmQvXF+AYOnaLPpc2qY5k7S4f33qMKDXzvudMIqkJuzV7WG9k0fRGlFsiCQ+j1BBUHLlR0ZvBYaAQHsdbG1VdmhpylKwplBfuqmi17PNaxVJq6THmujg4myd+t6mpKF+GF1jHN9n4Bss+kb3ww4++5nTXXxsIvNFuBH+sttSFY3u33+jnV1m3MbhM4mz7Hxoj0GS7buCBu/zPgG0XFIp99jp2fTdh0moO9MGar2NSSHI59GiGlqPtCa2ZHSlSzcqF3myEe7O+FUoj6drBhJnTInnG9Ou/qYloAUwo9HVLAlPTKh4qSelK0ZbZqqcuuRNTahPx7Zfkxu5av1J1SMXwYZI5U5J4PU81DeifhaQddvhfWM2PeLaXKb6tJgKR8yOHnGsY7gKDwT+8/I7j90jPOP/n9kTbWTJh0onLVLMDl6tk2cOIylpoJ7uQJ5K/hIlkcdq5H6IDEW02P21xJ0HWIlV3ciyhyWQwNfPTV8Pw4ANoiFtqxqiwHhQtLoIELD8KQYOu4SmztMJq4MYdwP1NWmHD1Nt506mGLSl8Q1DFYu4fF8"
`endif