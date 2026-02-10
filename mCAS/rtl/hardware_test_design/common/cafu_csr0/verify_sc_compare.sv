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

/*  Page 602 of CXL 2.0 Spec
*/

module verify_sc_compare
(
    input [511:0]   received_in,
    input [511:0]   expected_in,
    input [63:0]    byte_mask_reg_in,
    
    output logic [63:0]  compare_out
);
   
logic [511:0]  compare_z;


generate
genvar i;
    for( i = 0; i<512; i=i+1 )
    begin : gen_compare_z
        assign compare_z[i] = ( received_in[i] ^ expected_in[i] );
    end
endgenerate

generate
genvar j;
    /*
     *  do an binary OR of each byte to see if it has a mismatch but then
     *  AND that result with the byte mask index of that byte to see if 
     *  it is a byte that is enabled
     */
    for( j=0; j<512; j=j+8 )
    begin : gen_compare_out
       assign compare_out[j/8] = |compare_z[(j+7):j] & byte_mask_reg_in[j/8];
    end
endgenerate
    

endmodule

`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHF8QVUWWXHrTIxduBy/E4osOmhan3m5TbB5RkZ+Zow8zFPEMdVFtFIoiMNHf98cv7WP+t+PyC8+YH7NxQxThTqSVenROhn09mzTEazD361wUoqu94HHsallKr/1cZlE/uF9LmVYHMX3aQBKrsWNLFLQJylvESQ5W1/0xexH9RXpo4y0oA6vdzQkI2/lcG6pYfkeBNaLSJEJtHhHps6chMPy/Xc4/WL+6LU3E32LLUIy8m75MrZpmBQnwUsR3THO2JvCt3CWMlfZU8XEqbUuqeyBb9F6XVspWe6+83fZcBSMWjyB3MqtmfROo/EFk4vnzWg2GeSnTy74l9zoEK6vDDK8RPBX4a+d76UK63Z/PUQRKETOttOItly9nWFMxrN1gUnIGMPNo2NaQuRnuOTUb/Y4+eyFL8Jha/LteSAmaTzeue5S8m+8U2HOYvdgfLQ2z/bubnHQrbMjwuHvJ11Ici/K9akfKQ7IuzOH00rRvmpLjFvPyiso9bzIHGc9sXqu8e0XRLcNKaBIMs54RQS+KZ4p2A41+VIOByGIKafYpPQol51EN98/FrJeTLpycP0X1lc48B6nZ/gT/i8SW/qiH910TKe5EpT+aIdzWd7f8k/+ZJWCscKw2ByppxpaNjMJyxD1Hp1otXYPQ1S3YiyE6HLemPZWTxsoup9p+nNj5Y/+3LJIdTz+JzUzElzIxOLmE+oI5rGNX1gsaxbqagQlUHR8Y4D50lESScHYBmeX04JqkexBu7Zl4xjJzUkFaQvKHQ8Hbj1sIZCXRFfFHp2lS7Qj"
`endif