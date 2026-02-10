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


// (C) 2001-2016 Altera Corporation. All rights reserved.
// Your use of Altera Corporat's design                tools, logic functions and other
// software and tools, and its AMPP partner logic functions, and any output
// files any of the foregoing (including device programming or simulation
// files), and any associated documentation or information are expressly subject
// to the terms and conditions of the Altera Program License Subscription
// Agreement, Altera MegaCore Function License Agreement, or other applicable
// license agreement, including, without limitation, that your use is for the
// sole purpose of programming logic devices manufactured by Altera and sold by
// Altera or its authorized distributors.  Please refer to the applicable
// agreement for further details.

// synthesis translate_off
`timescale 1ns / 1ps
// synthesis translate_on

//`default_nettype none

module intel_pcie_reset_sync #(
  parameter                  WIDTH_RST              = 1
) (
  input                      clk,
  input                      rst_n,
  output [WIDTH_RST-1:0]     srst_n
);

  wire                       sync_rst_n;

  reg   [WIDTH_RST-1:0]      sync_rst_n_r /* synthesis dont_merge */;
  reg   [WIDTH_RST-1:0]      sync_rst_n_rr /* synthesis dont_merge */;

  assign srst_n              = sync_rst_n_rr;

  intel_std_synchronizer_nocut sync (.clk (clk), .reset_n (rst_n), .din (1'b1), .dout (sync_rst_n) );

  always @(posedge clk) begin
    sync_rst_n_r             <= {(WIDTH_RST){sync_rst_n}};
    sync_rst_n_rr            <= {(WIDTH_RST){sync_rst_n_r}};
  end 


endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwB5JV4P097Tr5mLctb+fSyqR4jwhXAZjhwjwRPl+URjg7MKFINlptc0EigU3ggV5lafa6QUc5Fao/igGUiUI0LPjjAUY+SAYNjqOKLO8K3rdy0gb+Tl5gLFbUXYFPvutE2xHkKWBQDREMEd6PBfCsiogW6enIrJomxt6tFUvWbo97PIBMC/EFMcWb9xiNrsAOd1UdyX5KfFp86E2C3u0EMKzQanbIZvuwVEWRKokhMA+hwBYznm0gecLsj7sC1pVEUV+1sIKGdTmMCbWcpg/xkV6iKzOL0c0+w6uPgtLqAIdsIssxyYvpLj5zmkffAq4BYqptYMzdVJGAdje8E+P/VcRLFcM+pna4OY9rjdimmjq9a2zaXRtoRt9FbwKe6oMXUcAdsMFW0hnkuDYm8s3ulWunuFrUGqrwdWdsEkJFODPToWRyq2KnqN4DVRdBDteuoKQMnTaJPseguXwnEO3uORJr2dACz9Uje+dtnUFgcS+nVM0G0brWltlTa21kFQ6srB19wsLYS8JFREtzbIuNyRkSa/OzQopAouSu8+fZCfVQp7X1Ko6WZ+aSVplyNRUVeSqhXxzULAqm/1JzHxkgyIlVI1u3hw/jRkpG7X9juyEo0SmcSXkAW9RuUlLnhTeSNaA2CsV7oaum/xSs78P8fi6aRhzOIdXIBX/tvb+hPEaH7GF7hJhS5JxgJjK0Scy4oZoCTf1T5Gov3Fr39kKhteSkrfJhFwhGAZpi1qpp/O011SFopU0PpnzRf7dcgOc3Z6Sz3ZLYtuivgmytu4XtyS"
`endif