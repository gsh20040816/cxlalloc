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

module fifo_sync_1
#(
   parameter DATA_WIDTH = 16,
   parameter FIFO_DEPTH = 16,
   parameter PTR_WIDTH  = 4,
   parameter THRESHOLD  = 10
)
(
  input                  clk,
  input                  reset_n,
  input [DATA_WIDTH-1:0] i_data,
  input                  i_write_enable,
  input                  i_read_enable,
  input                  i_clear_fifo,  // should come from top level set to busy
  
  output logic [DATA_WIDTH-1:0] o_data,
  output logic                  o_empty,
  output logic                  o_full,
  output logic [PTR_WIDTH-1:0]  o_count,
  output logic                  o_thresh
);

logic [DATA_WIDTH-1:0] fifo_ram [FIFO_DEPTH-1:0];

logic [PTR_WIDTH-1:0] write_ptr;
logic [PTR_WIDTH-1:0] read_ptr;


always_comb
begin
    o_empty  = (o_count == 0);
    o_full   = (o_count == (FIFO_DEPTH-1));
    o_thresh = (o_count > (THRESHOLD-1));
end


always_ff @( posedge clk )
begin
  if( (reset_n == 1'b0) 
    | (i_clear_fifo == 1'b1) ) begin
                               o_count <= 'd0;
  end
  else if( (o_full == 1'b0)
         & (i_write_enable == 1'b1)
         & (o_empty == 1'b0)
         & (i_read_enable == 1'b1) ) begin
                               o_count <= o_count;
  end
  else if( (o_full == 1'b0)
         & (i_write_enable == 1'b1) ) begin
                               o_count <= o_count + 'd1;
  end
  else if( (o_empty == 1'b0)
         & (i_read_enable == 1'b1) ) begin
                               o_count <= o_count - 'd1;
  end
  else                         o_count <= o_count;
end


always_ff @( posedge clk )
begin
  if( (reset_n == 1'b0) 
    | (i_clear_fifo == 1'b1) ) begin
                               o_data <= 'd0;
  end
  else if( (o_empty == 1'b0)
         & (i_read_enable == 1'b1) ) begin
                               o_data <= fifo_ram[read_ptr];
  end
  else                         o_data <= o_data;
end


always_ff @( posedge clk )
begin
  if( (o_full == 1'b0)
    & (i_write_enable == 1'b1) ) begin
                               fifo_ram[write_ptr] <= i_data;
  end
  else                         fifo_ram[write_ptr] <= fifo_ram[write_ptr];
end


always_ff @( posedge clk )
begin
  if( (reset_n == 1'b0) 
    | (i_clear_fifo == 1'b1) ) begin
                               write_ptr <= 'd0;
  end
  else if( (o_full == 1'b0)
         & (i_write_enable == 1'b1) ) begin  
                               write_ptr <= write_ptr + 'd1;
  end
  else                         write_ptr <= write_ptr;
end


always_ff @( posedge clk )
begin
  if( (reset_n == 1'b0) 
    | (i_clear_fifo == 1'b1) ) begin
                               read_ptr <= 'd0;
  end
  else if( (o_empty == 1'b0)
         & (i_read_enable == 1'b1) ) begin  
                               read_ptr <= read_ptr + 'd1;
  end
  else                         read_ptr <= read_ptr;
end

endmodule
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "8PhOGCnqQbAbJgmQKuGahsXyBGOqnzJpaaepB4em/LfHKuzJoKpoYsBe35LjKDi25jgan/aauEWsf8HDTdfj7jcC2yCsixDCQ3p7OCgc9Hp1H2OzKOvi1qhwBi7kYnvuqWX26D6nOTtEFDfggOZVro8JXoRIL9p3vh13LBiTXTGn8SWhyyzJS6Tqcxi2IXTDzRrzmeGJnXTRhMERLiUIGnZaBs86Pr/nV4RjvtzkvHFyr3n2sVzpBqLTKOKqvv+/YJiX3EwBK8nwOyNCpcRrYl3DLy+7xvImrtSzQNamO9RyW+qsBnPTD63WHCkTfkmKIbhR6IJn1nBg+aOjhyOCK2yjLVteZIkQNMXIRzHjlN+gdtb53mhyuF5T7u6cgQ5toKyi8H4CPAR175P2HQgmE3PhWdPaGmI1AZmEewQ6h2Tn71BpTKBsO54lNyUoIIYdD+NGYN7gnsHxGQtzR9KtkonWb6NpQ1u7uRq8na2cjGxx17ngvZ1qrFJXYgryO+/vUHEspjJ3Qfxqte2+nCXUTWH35YLLYdn7o8Q9c4s8nO9sq026ef14suN/xaTYZ1zQ0HC8wJ3pINnShjE3i6YEV4A/2QMDLqQL4Po8DIi57ri5zqExhK2RwVAbE0LdBl5KFQIvJ3b2KNxtm9jljg6crkQzjt65YfZR3gcslkBtUv3nbEVaEha7rMSTAEphXYSQgp9WWmocNRln7jvdw4AEguqO0l3O4s9ODFZh4is123aXiPfpHW6JM7XePsigS6MkBzpxR+aVgWsIPBnpqn+lJrcOcJYU3zsHWOOoXfURVk05XP9hdpYYwPb7aPfwCuIjoJpsbABV+DPIkWQ3fHG0tLkq3/ZqrkWTAiqawgRSZT2WB5u17Eu4mAsYHMELBcxAW6MJa3/DNw7c3Ybef2dFl7HwVVv3cE33HLrA29KVoT9tm9ajqR6kkkYFBpOipVHTbfSu2Y1v7lSn21zmNkwPiXcW0Sd7FsIH2wWUz+vZcU/vic8ZDADTxhuoqGDniiku"
`endif