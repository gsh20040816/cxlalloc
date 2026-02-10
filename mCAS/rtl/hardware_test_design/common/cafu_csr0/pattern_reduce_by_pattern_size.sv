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


// Copyright 2022 Intel Corporation.
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

/*  Page 603 of CXL 2.0 Spec

PatternSize: Defines what size (in bytes) of P or B to use starting from 
least significant byte. As an example, if this is programmed to 3b011, 
only the lower 3 bytes of P or B registers will be used as a pattern. 
This will be programmed consistently with the ByteMask field and the base
address.
*/

module pattern_reduce_by_pattern_size
(
   input [2:0]           pattern_size_reg_in,
   input [31:0]          pattern_in,

   output logic [31:0]   pattern_out
);


always_comb
begin
  case( pattern_size_reg_in )
    // 3'b100 : pattern_out = pattern_in;
    3'b011  : pattern_out = {8'd0, pattern_in[23:0]};
    3'b010  : pattern_out = {16'd0, pattern_in[15:0]};
    3'b001  : pattern_out = {24'd0, pattern_in[7:0]};
    3'b000  : pattern_out = 32'd0;
    default : pattern_out = pattern_in;
  endcase
end



endmodule


`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHFVwdEPjRhctZmoL3agVmhpDS4iAid/1ZhP7I+183FIRJhvPUzXdsk8VgttWbKVVga09vifiKKPPXPxhBNg91KoW2d0D9SGyY+K98+k9i8tg5Vr/AWNXPXcDMP9w8Vs2+tWFgjZXtqY3oq/cAYHgrRUjVRo0FfQSHvo1JMSSqP76P+u1Jb9vjbQK6OKm6V5+pwOhzpi/rZv+bP2XYCTUDpTasmSPCNOVCF8daknUjzKLwOAOxHtzg35d+9D7nf1R4HtocvuFRtmCSI2s1Y89akjGG+wkVte2A27LRwebePgoTy8UyS4Up01MlqV19uoNoxt/WWV4k+5XmvS54U8ddVoydOPlm/n+gY/NsfKZBu3vpy9Ov/va8yLKBzFObgVjbXdkAR8PahEEjKO/x6lYXSYZIuRWGva2pvWjZJil26SSGPYWSukx+b8+cigmYN118+NZft5ee6j/4qz+68Z+6EqeSo7gtjKh5t9rLpMLAnO5M+djs12Qlk0AVohRWcw8C/KveZVw8x47Rsi4y2vGUTJY7Qk+33LbIIR6J30ntCiu2R/Ew0rJzO4gypXMc/vZy4vTQcrrXXK2C55R7Y6Dn2WHQIRrqJEqOxpTI2Ky9aZ84MD9LOLeUJDq7K+QOwN0WOuvkfyN5IuXRzBcTl+o/e22fITyKaJYnXxoA1AbS4eeTzqQS/YV+JpGT2xCKv5bkTwHJyuQjFZ7Bnr9BqmFJi9JLimp/bmnHWxyhPftoK3+P/YSGF/huGcPQnBi7MbTxrtkTFDb6O7rR334URApqNu"
`endif