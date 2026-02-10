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

module afu_ate_comparator #( parameter DATA_WIDTH = 512'd512  )(
   input                             rtl_clk,
   input           [DATA_WIDTH-1:0]  dataa,
   input           [DATA_WIDTH-1:0]  datab,
   input     logic                   enable,
   output    logic                   eq
   );

 
 generate  
      if(DATA_WIDTH < 4) begin
          always @(posedge rtl_clk) begin
            if(!enable) begin
               eq <= 1'b0;
             end
             else begin
               eq <= (dataa == datab);
             end
          end
        end else begin
          localparam WIDTH_BY_4=DATA_WIDTH/4;
          wire eq0,eq1,eq2,eq3;
          afu_ate_comparator #(.DATA_WIDTH(WIDTH_BY_4)) c0 (.rtl_clk(rtl_clk),
                                                       .dataa(dataa[WIDTH_BY_4-1:0]),
                                                       .datab(datab[WIDTH_BY_4-1:0]),
                                                       .enable(enable),
                                                       .eq(eq0)
                                                      );
          afu_ate_comparator #(.DATA_WIDTH(WIDTH_BY_4)) c1 (.rtl_clk(rtl_clk),
                                                       .dataa(dataa[2*WIDTH_BY_4-1:WIDTH_BY_4]),
                                                       .datab(datab[2*WIDTH_BY_4-1:WIDTH_BY_4]),
                                                       .enable(enable),
                                                       .eq(eq1)
                                                      );
          afu_ate_comparator #(.DATA_WIDTH(WIDTH_BY_4)) c2 (.rtl_clk(rtl_clk),
                                                       .dataa(dataa[3*WIDTH_BY_4-1:2*WIDTH_BY_4]),
                                                       .datab(datab[3*WIDTH_BY_4-1:2*WIDTH_BY_4]),
                                                       .enable(enable),
                                                       .eq(eq2)
                                                      );
          afu_ate_comparator #(.DATA_WIDTH(WIDTH_BY_4)) c3 (.rtl_clk(rtl_clk),
                                                       .dataa(dataa[4*WIDTH_BY_4-1:3*WIDTH_BY_4]),
                                                       .datab(datab[4*WIDTH_BY_4-1:3*WIDTH_BY_4]),
                                                       .enable(enable),
                                                       .eq(eq3)
                                                      );
        always  @(posedge rtl_clk) begin
           if(!enable) begin
              eq <= 1'b0;
            end
            else begin
              eq <= eq0 & eq1 & eq2 & eq3;
            end
        end                                                               
        end
endgenerate 

    

endmodule

`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "2M3FFSmZRo+OLDXkYi9QgRKIMy2+kBf8+s3s4c/izvMHLMxClhU2h++VjBIj3jsb5ZYIvLYWFsScMuG6+Wc4dsRtKcSj6DjH0dACKmVLAW04huWOolapjZ+Qsree1nouVGRaWBxKtEz7Pc+upAJSBmn/6rKL+D4mnksigk9L8DhC5yXuN7JKuDl1odnnzvJcc/Z8zZ3EENdv8TME2Fpb1a4B0w6wIbaMCHaK7JxR2QV/qOfTehIR497ivsSbSAJKi4iivsax2tfJx7Pdb4xEWzSD8pc94Qjjd0CXa8ZmHWUxtzsV2CyxZHMGfUAoBJAiJwOuvvKiWGte/fT+ljzQ1/NrSNMn5CfeNySpMYqhetxi3DIms0bXtNOoOdagDA7o8FExz3O3owkquE8SRI7PLld7xafREV2ykQqkFbhNYAKB1J6oJpaUr/1nZZ6mXuQIX8wSKxTcYbuRtLt9T7JBsBZ+nkqexxDwzlDcQISJw13OpxfQm5OEjTc8wl1NieeAcHGWZszbuiEarKB9K86K1Z3y5fYDYsK1xn/QexYmYU15GrGIEEpRDK9SESxStBXIwVCH1ckj5xapw/N9HTyRCNfBGxUrnGwMgXHTFY3KQ1mtvXKni5OKuYKcjV++zDB4DCNFzcjZA5OsjI+aAL7hb1oCLc6Gy+V6VfZI9P/qUVgDPw6JOmmMir+gFtpoVwV2WmakFGt/ugFLDTpndHw8l9lKUPk//Xs1WTPX5YP2NgnezABJ7o9s58I2XUOBmswvC7LAAKCk3MSbmVbXLS4aDmtVQCRNQiv6FsNKNHdLcgz9zYv4pFGg+p3NL+lx5gCDDXeLT4SngOCEcDkTqK1gTCGHB6TMsGaUP4LWy1cTygZyeK7vxiQMQR74HjZLfCy2RJRgWNobU2dAFtrEyrJ9St0XKm3q67QSk9GrENZoA8nD+WbbKUgSTHym+vhV02PelqAl75KNiUJl+U9+bxb+3P6ijeaG0o9qriBsmE3QiNiKN0Do4ui/DwRj8nI2+fph"
`endif