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
// Description: Generic RAM with one read port and one write port.
//              Write port includes byte enables.
//

module cafu_ram_1r1w_be (clk,    // input   clock
                         we,     // input   write enable
                         be,     // input   write ByteEnables
                         waddr,  // input   write address with configurable width
                         din,    // input   write data with configurable width
                         raddr,  // input   read address with configurable width
                         dout    // output  write data with configurable width
                        );

parameter BUS_SIZE_ADDR = 4;                  // number of bits of address bus
parameter BUS_SIZE_DATA = 32;                 // number of bits of data bus
parameter BUS_SIZE_BE   = BUS_SIZE_DATA/8;
parameter GRAM_STYLE    = "no_rw_check, M20K";


input                           clk;
input                           we;
input   [BUS_SIZE_BE-1:0]       be;
input   [BUS_SIZE_ADDR-1:0]     waddr;
input   [BUS_SIZE_DATA-1:0]     din;
input   [BUS_SIZE_ADDR-1:0]     raddr;
output  [BUS_SIZE_DATA-1:0]     dout;

//Add directive to don't care the behavior of read/write same address
(*ramstyle=GRAM_STYLE*) reg [BUS_SIZE_BE-1:0][7:0] ram [(2**BUS_SIZE_ADDR)-1:0];  //ram divided into bytes.

reg [BUS_SIZE_ADDR-1:0] raddr_q;
reg [BUS_SIZE_DATA-1:0] dout;
reg [BUS_SIZE_DATA-1:0] ram_dout;
/*synthesis translate_off */
reg                     driveX;         // simultaneous access detected. Drive X on output
/*synthesis translate_on */


always_ff @(posedge clk)
  begin
    if (we)
      for (int i=0; i<BUS_SIZE_DATA/8; i++) 
      begin
        if (be[i])                
          ram[waddr][i]  <= din[7+(8*i)-:8];  // synchronous RAM write with byte enables
      end
    ram_dout<= ram[raddr];
    dout    <= ram_dout;
    /*synthesis translate_off */
    if(driveX)
      dout    <= 'hx;
    if(raddr==waddr && we)
      driveX <= 1;
    else
      driveX <= 0;            
    /*synthesis translate_on */
  end


endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHELYgS2QWQmyU9p9M5eK0tTFQTDuVK3Zpqdg0vsUxe41/UPSvMNOr8gRGZmZE6USdy3lclAiLjdhVVxL2GijT4yX+9Uzi6aYGJdtvo7d7v2dvsBNj2zolhUj+h3wNE6y5/H/r5INGknqfH11DolgIy70K3le9i0LL++enc1e9xXCIis/LplDHYKpb7mGXTlTVKAWwhAKiCBsdk5pWDB4KrjTs/OWkan8KrVzrkuv1TAN5KpRiDJ2XYojE+fsrRrqomUiY0EeAxY9ZIW18q1i1BQekGzHzclLYYpCYaMAdRgzbGxNTGfV61id8yMJLUvWwP74SIW3Q37pK4jFxwroJtY0yU3uj5L/pObfstOkDOikLnwVwFrO7OcXqrxxEvuCuLM4anPM+Q8Os03/bS66vFATtfcmpu3geIYV5EaGzixwkoJPWnf95RD/qXq/7Vz3wVb2Z8wUGzngDTkS6XBgH+1sg+L5dkdZT2kRiyVlnUpUhVjbvVr0yOnyq6GFRNNBuxsJPn0ejvCr07H7xh6gHUXcQQv3C2XE6DsY1SaeINOo0dp2s7K7gnGCVYspbgKelxFOPLhFXwiFgpR5CHDOiIUlov4me5EC6jnqycXp1hzZrnKSTTf1vJyG4UT0e+Fcglo6aD5yyXamockiGIjMk6P6Owc1qn3Vl2c/fYHZVjfMOVOYnt486NoJiI4QP1dq0n4dyC5nuvd8BQwvOYr9BC19sKf1PbFLsCGIzJDCUPhxEOzWePFYr+eAr+2ngy5koMzBiGZtd5YeRyDFM2oY+Nn"
`endif