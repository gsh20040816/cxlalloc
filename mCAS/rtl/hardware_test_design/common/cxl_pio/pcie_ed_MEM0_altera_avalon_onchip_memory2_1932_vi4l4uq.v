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


//Legal Notice: (C)2023 Altera Corporation. All rights reserved.  Your
//use of Altera Corporation's design tools, logic functions and other
//software and tools, and its AMPP partner logic functions, and any
//output files any of the foregoing (including device programming or
//simulation files), and any associated documentation or information are
//expressly subject to the terms and conditions of the Altera Program
//License Subscription Agreement or other applicable license agreement,
//including, without limitation, that your use is for the sole purpose
//of programming logic devices manufactured by Altera and sold by Altera
//or its authorized distributors.  Please refer to the applicable
//agreement for further details.

// synthesis translate_off
`timescale 1ns / 1ps
// synthesis translate_on

// turn off superfluous verilog processor warnings 
// altera message_level Level1 
// altera message_off 10034 10035 10036 10037 10230 10240 10030 13469 16735 16788 

module pcie_ed_MEM0_altera_avalon_onchip_memory2_1932_vi4l4uq (
                                                                // inputs:
                                                                 address,
                                                                 byteenable,
                                                                 chipselect,
                                                                 clk,
                                                                 clken,
                                                                 freeze,
                                                                 reset,
                                                                 reset_req,
                                                                 write,
                                                                 writedata,

                                                                // outputs:
                                                                 readdata
                                                              )
;

//  parameter INIT_FILE = "pcie_ed_MEM0_MEM0.hex";


  output  [1023: 0] readdata;
  input   [  7: 0] address;
  input   [127: 0] byteenable;
  input            chipselect;
  input            clk;
  input            clken;
  input            freeze;
  input            reset;
  input            reset_req;
  input            write;
  input   [1023: 0] writedata;


wire             clocken0;
wire             freeze_dummy_signal;
reg     [1023: 0] readdata;
wire    [1023: 0] readdata_ram;
wire             reset_dummy_signal;
wire             wren;
  assign reset_dummy_signal = reset;
  assign freeze_dummy_signal = freeze;
  always @(posedge clk)
    begin
      if (clken)
          readdata <= readdata_ram;
    end


  assign wren = chipselect & write;
  assign clocken0 = clken & ~reset_req;
  altsyncram the_altsyncram
    (
      .address_a (address),
      .byteena_a (byteenable),
      .clock0 (clk),
      .clocken0 (clocken0),
      .data_a (writedata),
      .q_a (readdata_ram),
      .wren_a (wren)
    );

  defparam the_altsyncram.byte_size = 8,
//           the_altsyncram.init_file = INIT_FILE,
           the_altsyncram.lpm_type = "altsyncram",
           the_altsyncram.maximum_depth = 256,
           the_altsyncram.numwords_a = 256,
           the_altsyncram.operation_mode = "SINGLE_PORT",
           the_altsyncram.outdata_reg_a = "UNREGISTERED",
           the_altsyncram.ram_block_type = "AUTO",
           the_altsyncram.read_during_write_mode_mixed_ports = "DONT_CARE",
           the_altsyncram.read_during_write_mode_port_a = "DONT_CARE",
           the_altsyncram.width_a = 1024,
           the_altsyncram.width_byteena_a = 128,
           the_altsyncram.widthad_a = 8;

  //s1, which is an e_avalon_slave
  //s2, which is an e_avalon_slave

endmodule

`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwAXR1HFjA87IPMQpv6E3z29rlJgUsy0Xj9BBl101qjxHIDxrrUctzsfcLh0Zh62Yr2H0KALXh0cJr4xGgSTCjhNdn2dOPaLkzMQ3wlLnLSb52qXB6CqGZfm5yzzCWkVUblknJzf0eftXwi3WbRW1sxRIJ7IaReRGoZVfK/x2bizetLTptqZo/kRbZkDQWWyP5YNp/NoM+tSVs/ZuY1pn5yq76NvH+cGj7eWX/x1HmnpRoqh7AEpbqoWkYDS4fJoYZoRsZaxAvB0v5FikbArz24SxU24GZeZZu+k3R0BI0Raqk/r+5JBx65jkAQi0MMX2TUcC8pSb+3F6aQS/HUnKM3IK5VdlOIczi+p+tU7ru9ySM9DKGTSUX7vj0kJX1KOxFpXWRWrNdfJi18D1rdL4ToX+ujQWOONaNH/89Qgwc7b118dQS5rjGUvjvD25CCGxXRu2lAOmgy1mj/er6kAtg783FbqSBFv0N9hxpS76cLMbFkNId2VlV0zJ6DG9rdlpwWelZc1271tIUILQtIvePuloiNlgInEjmqejnDAACyUvFaZbV6njMMhJTMRwDITm1pqvibAhyJl4rTHnX2wG2WG3EF+pu4E3475Ko3RQLr3wZaDwbPns8//Et7eQe4+KOfsYbHKsZ3iNuAOcV0ASQH4YYCbHgnUwGZ6LSttxoerQdAAXye1UZk6s6X+YbWULLyh6/Al061lJ3tFXa0ZWmYHePgb+dY9lNNotuuFCAeiNjKhZb9ouNVHstE7l17vSnVV94MjxhCxGGgGv06QLYGL"
`endif