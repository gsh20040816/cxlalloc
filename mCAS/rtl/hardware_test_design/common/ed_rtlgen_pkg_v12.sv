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


///
///  INTEL CONFIDENTIAL
///
///  Copyright 2022 Intel Corporation All Rights Reserved.
///
///  The source code contained or described herein and all documents related
///  to the source code ("Material") are owned by Intel Corporation or its
///  suppliers or licensors. Title to the Material remains with Intel
///  Corporation or its suppliers and licensors. The Material contains trade
///  secrets and proprietary and confidential information of Intel or its
///  suppliers and licensors. The Material is protected by worldwide copyright
///  and trade secret laws and treaty provisions. No part of the Material may
///  be used, copied, reproduced, modified, published, uploaded, posted,
///  transmitted, distributed, or disclosed in any way without Intel's prior
///  express written permission.
///
///  No license under any patent, copyright, trade secret or other intellectual
///  property right is granted to or conferred upon you by disclosure or
///  delivery of the Materials, either expressly, by implication, inducement,
///  estoppel or otherwise. Any license under such intellectual property rights
///  must be express and approved by Intel in writing.
///

`ifndef CFGPKG_v12
`define CFGPKG_v12

package ed_rtlgen_pkg_v12;

function automatic logic [7:0] f_sai_sb_to_cr (
   input logic [7:0] sai_sb
);
   if (sai_sb[0] == 1) begin 
      if (sai_sb[7:4] != 4'b0000) begin
         f_sai_sb_to_cr = ({2'b00, 6'b111111});
      end
      else begin
         f_sai_sb_to_cr =  ({2'b00, 3'b000, sai_sb[3:1]});
      end
   end
   else begin 
      if (sai_sb[7:1] > 7'b0000111 && sai_sb[7:1] < 7'b0111111) begin
         f_sai_sb_to_cr =  ({2'b00, sai_sb[6:1]});
      end
      else begin 
         f_sai_sb_to_cr = ({2'b00, 6'b111111});
      end
   end  
endfunction : f_sai_sb_to_cr

// @@copy for common_afu_pkg@@start
typedef enum logic [3:0] {
    MRD   = 4'h0,
    MWR   = 4'h1,
    IORD  = 4'h2,
    IOWR  = 4'h3,
    CFGRD = 4'h4,
    CFGWR = 4'h5,
    CRRD  = 4'h6,
    CRWR  = 4'h7
} cfg_opcode_t;

localparam CR_REQ_ADDR_LEN = 48;
localparam CR_REQ_ADDR_HI = 47;

localparam CR_MEM_ADDR_HI = 47;
typedef struct packed { // 48
    logic [CR_MEM_ADDR_HI:0] offset;
} cfg_addr_mem_t;

localparam CR_IO_ADDR_HI = 15;
typedef struct packed { // 32+16=48
    logic [31:0] pad;
    logic [CR_IO_ADDR_HI:0] offset;
} cfg_addr_io_t;

localparam CR_CFG_ADDR_HI = 11;
typedef struct packed { // 36+12=48
    logic [35:0] pad;
    logic [CR_CFG_ADDR_HI:0] offset;
} cfg_addr_cfg_t;

localparam CR_MSG_ADDR_HI = 15;
typedef struct packed { // 32+16=48
    logic [31:0] pad;
    logic [CR_MSG_ADDR_HI:0] offset;
} cfg_addr_msg_t;

localparam CR_CR_ADDR_HI = 15;
typedef struct packed { // 32+16=48
    logic [31:0] pad;
    logic [CR_CR_ADDR_HI:0] offset;
} cfg_addr_cr_t;

typedef union packed { // All structs must be 48
    cfg_addr_mem_t mem;
    cfg_addr_io_t  io;
    cfg_addr_cfg_t cfg;
    cfg_addr_msg_t  msg;
    cfg_addr_cr_t  cr;
} cfg_addr_t;

// ==========================================================================

// for 64bit bus
typedef struct packed { 
    logic        valid;
    cfg_opcode_t opcode;
    cfg_addr_t   addr;
    logic  [7:0] be;
    logic [63:0] data;
    logic [7:0] sai;
    logic  [7:0] fid;
    logic [2:0] bar;
} cfg_req_64bit_t;

typedef struct packed {
    logic        read_valid;
    logic        read_miss;
    logic        write_valid;
    logic        write_miss;
    logic        sai_successfull;
    logic [63:0] data;
} cfg_ack_64bit_t;
// @@copy for common_afu_pkg@@end

// for 32bit bus
typedef struct packed { 
    logic        valid;
    cfg_opcode_t opcode;
    cfg_addr_t   addr;
    logic  [3:0] be;
    logic [31:0] data;
    logic [7:0] sai;
    logic  [7:0] fid;
    logic [2:0] bar;
} cfg_req_32bit_t;

typedef struct packed {
    logic        read_valid;
    logic        read_miss;
    logic        write_valid;
    logic        write_miss;
    logic        sai_successfull;
    logic [31:0] data;
} cfg_ack_32bit_t;

// for 8bit bus
typedef struct packed { 
    logic        valid;
    cfg_opcode_t opcode;
    cfg_addr_t   addr;
    logic  [0:0] be;
    logic  [7:0] data;
    logic [7:0] sai;
    logic  [7:0] fid;
    logic [2:0] bar;
} cfg_req_8bit_t;

typedef struct packed {
    logic        read_valid;
    logic        read_miss;
    logic        write_valid;
    logic        write_miss;
    logic        sai_successfull;
    logic  [7:0] data;
} cfg_ack_8bit_t;

// ==========================================================================
// Merge ack from multiple CR banks
typedef struct packed {
   logic treg_trdy; 
   logic treg_cerr;   
   logic treg_rdata;
} cr_bank_ack_t;

// ==========================================================================

endpackage: ed_rtlgen_pkg_v12

`endif
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EknmHwQp1Im7e42VTEW4oP2YlkjZ4q1qxNf0pFfjUqCitHkH39CYcFkwSiDngDquknlwOs0cV09OWg1Vqr9AJzKHl9u3jmr1ART1KkS8wXukVlgHSVfd16n8/T9v5gAWwkVERtgG7ZXREFE2Ma4KnGT/aFGmA6j1B8XDnuCKFAsk/YDTkJMsrKEHO5zzdNk6r7SaThxY+E0ejgfrwMULALf2OnQu+lk4HNYpq3cSGSSqoBKTZSLHadL1YgDIrMc135TwrTfTXXCbRy8HCyKj0blkQN/r/roUKVEMcX3hv0G7MVuudI3deI+z6Y/JFdD0ufV59bGfHOPAfo1zqk5KOXlm35s6IVnbBfdda/Ns+D7KdgtYubfT2wh8n1XbzpqF/VuIGOVccHvDi9W4Jb7zO8cCVQB3SBlqcyM2fXaLv8cJApi6m+5QLhWPyM5LI4Szo2XiXvN7eg8IkWMEcWVGNnVKm0TIAaazrQe9W8H63ZpsweT1+wCsg2pUD/zgG3PzC6g4dWpZN+dHVbrBAWvnmjjZ4n3kuCb9bf1L+noDQY9GGso4T7ZWpS+z7b1JQkFGQeuk2w1pcLwDZ1Ne/4c3BY2TRY2qZXKmbfBrS9srewuEH8ylfxsSM4HDi0ILUUBGx0E+CByQAS3KUU43GcCzsoMBpkA1CemCOnQD3H5SFsMOmUsOIfo9CXd9UGleWFpKwkDAeMQ6R5tzdd02qXENHFGX7klDyXsZLBRbNoLFe/Wy9k4dE2RIMxpRoLw9LpwbrxgTp7wwUOdY1AM5eX7nNuq+8oN9j6BurVWfnNjAP8KwYbJrghGl0mmup/KID6VKTUJc75bBjZNBDlTWANNqm/hRNAJ/U5/2frTS5yy2lxBs3hMRzWCR8ARPLBQyT1YrYS39RtZYkW1jZCU4LyUzsoBoNjDSUizsktFvxx0TRcQ9S0wKGYqUdaEQfcBC4E0BSf1RtSUNaHJRT1b6oeUiy2ytTxWTK2G1b8fHaQDbZbXT6t0haur7hpEvRNczi80s"
`endif