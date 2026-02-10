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


// -------------------------------------
// Merlin Demultiplexer
//
// Asserts valid on the appropriate output
// given a one-hot channel signal.
// -------------------------------------

`timescale 1 ns / 1 ns

// ------------------------------------------
// Generation parameters:
//   output_name:         pcie_ed_altera_merlin_demultiplexer_1921_s5kn7vi
//   ST_DATA_W:           1267
//   ST_CHANNEL_W:        1
//   NUM_OUTPUTS:         1
//   VALID_WIDTH:         1
// ------------------------------------------

//------------------------------------------
// Message Supression Used
// QIS Warnings
// 15610 - Warning: Design contains x input pin(s) that do not drive logic
//------------------------------------------

// altera message_off 16753
module pcie_ed_altera_merlin_demultiplexer_1921_s5kn7vi
(
    // -------------------
    // Sink
    // -------------------
    input  [1-1      : 0]   sink_valid,
    input  [1267-1    : 0]   sink_data, // ST_DATA_W=1267
    input  [1-1 : 0]   sink_channel, // ST_CHANNEL_W=1
    input                         sink_startofpacket,
    input                         sink_endofpacket,
    output                        sink_ready,

    // -------------------
    // Sources 
    // -------------------
    output reg                      src0_valid,
    output reg [1267-1    : 0] src0_data, // ST_DATA_W=1267
    output reg [1-1 : 0] src0_channel, // ST_CHANNEL_W=1
    output reg                      src0_startofpacket,
    output reg                      src0_endofpacket,
    input                           src0_ready,


    // -------------------
    // Clock & Reset
    // -------------------
    (*altera_attribute = "-name MESSAGE_DISABLE 15610" *) // setting message suppression on clk
    input clk,
    (*altera_attribute = "-name MESSAGE_DISABLE 15610" *) // setting message suppression on reset
    input reset

);

    localparam NUM_OUTPUTS = 1;
    wire [NUM_OUTPUTS - 1 : 0] ready_vector;

    // -------------------
    // Demux
    // -------------------
    always @* begin
        src0_data          = sink_data;
        src0_startofpacket = sink_startofpacket;
        src0_endofpacket   = sink_endofpacket;
        src0_channel       = sink_channel >> NUM_OUTPUTS;

        src0_valid         = sink_channel[0] && sink_valid;

    end

    // -------------------
    // Backpressure
    // -------------------
    assign ready_vector[0] = src0_ready;

    assign sink_ready = |(sink_channel & ready_vector);

endmodule

`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwD29HeUUBcg9tiSo2BcBFHig2AmcxH2uorRWRM3q8eXhji4qurLhfKa/vTSDBWzS/ia1HRrFMn8G8/O7V3bugWndkd4r5G7blDe+zIqyGVSEh7/DCcHJK/bw1LjsJNxN3FY7X/G7iAdyylPUZIxiDlh/Wna8bmdeIkjAKW6R+a5wk6DBublmeMcW6rTfsrs9sW6xdCDQFu3wXgLbiZ0dyfDVAxRVZgqAj39aER+qxLXPBaBRrzT8NO7rV5dD89F/jqOWzaa8Ps/GAYuEQxgmcY6Koh84Slxa33gQd1O6AaMs34dKMvCOHAoPF6Epn8/ACSBj58FlLd2IhKRgkRbTzX5rLBWA6DsmLKa4XsN8hdupZtfwOcyGHg9dppfCSLK9BI27B4/7pQJG5Tut0KoHx8zpcOlIrQ4g+MSxn7LNWZGtItFxK/qRVDMcTqirb9uY+4cfAqos7xzosndRLlVw2UlOW9QWeC+c7CAfV25HqnuvF/34zU+sYl1sWdmy4X7RA8HkBf5A2+T8riuk726TyiUkfXPhnejOkj46ikx9isBgYi9p6VK+nql/MTyuMkFUwTR8C9DNF+w3P9/5RmDZrRqbOAv+Dpv3Ng7yVBESjqQcnPp8dZ/FyxtsJc4t+Uwi5qosia4rxbhGJ4DtgPO6lS03t86OxzLScgY8XLi0U9H0P96MyANANULb4R0eFtZ0GqAjlANrOU8k1mEc8MN0lF11CJpgQQKAyptz/q+4Sc8RLT1YTCTloh+WMg8xfYp4Pmu+jHPv6kTtiTI3qhgtTSZ"
`endif