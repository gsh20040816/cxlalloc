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


package afu_ate_mem_target_pkg;
    
    localparam  CL_ADDR_MSB = 51;
    localparam  CL_ADDR_LSB = 6;    
    
    typedef logic [CL_ADDR_MSB:CL_ADDR_LSB]        Cl_Addr_t;
    
    typedef struct packed {
        logic [CL_ADDR_MSB:28]  Addr;
        logic [CL_ADDR_MSB:28]  Size;
        logic [3:0]             IW;
        logic [3:0]             IG;
    }  hdm_mem_base_t;  //used for address decode in fabric_slice 
    
    typedef enum logic {
       TARGET_HOST_MEM     = 1'b0,
       TARGET_DEV_MEM      = 1'b1
    } fabric_target_dcd_e;    
    
    function automatic fabric_target_dcd_e fabric_target_dcd_f;
        input Cl_Addr_t        Addr;
        input hdm_mem_base_t   Base;
    
        localparam ADDRMATCH1  = 'h0_0000_0004;
        localparam ADDRMATCH2  = 'h0_0000_0005;
    
        logic [CL_ADDR_MSB:28]      shifted_addr;
    
        //shifted_addr = Addr << 22; //since CL Addr, shift 22 instead of 28
        shifted_addr = Addr[CL_ADDR_MSB:28];
    
        if ( (shifted_addr[CL_ADDR_MSB:28] <   Base.Addr + Base.Size)
           & (shifted_addr[CL_ADDR_MSB:28] >=  Base.Addr))
            fabric_target_dcd_f = TARGET_DEV_MEM;
        else
            fabric_target_dcd_f = TARGET_HOST_MEM;  
    
    endfunction    
    

endpackage

`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "2M3FFSmZRo+OLDXkYi9QgRKIMy2+kBf8+s3s4c/izvMHLMxClhU2h++VjBIj3jsb5ZYIvLYWFsScMuG6+Wc4dsRtKcSj6DjH0dACKmVLAW04huWOolapjZ+Qsree1nouVGRaWBxKtEz7Pc+upAJSBmn/6rKL+D4mnksigk9L8DhC5yXuN7JKuDl1odnnzvJcc/Z8zZ3EENdv8TME2Fpb1a4B0w6wIbaMCHaK7JxR2QXlg1oZgOYyN6VkZFMAnVCwTwA04tydAGDN0kBchYBdrDpR0bvvdS8dHdXskqX3ahlcJLUV2I3+3dd/eMSxGjaw6XSwIA00JMXjaudWoMXsBQykopIqYOy14MgIvXLlox43QctzW3O48w9hIjNS1Dd6N6xPPcoZUrnIG/DgVMhwZw9VXJo+c6GUE/VlPeybRbP/rzt3z8EoWh256UTEgFRuCLZ4boh+oY09AGduKFu0D/kyo4I30b1e2UrU8lXjWAZcTEqfFZkEJruasJphSj1GNOrBT0lSnWNscAR7bBP8pvLjlnEC3IzBzCtiGtFcoYEU//pYPE4/U64JlElEbguoU18EambypGDLCOAJxBcTGRW35CQjnL+63HvrDju8pSuxvuh+yC4j3v4zlULnH5nVLiKRo0iGN4Fvaja9MNI84J2RHs16O4eD2SeqlAL1XA9MXFq0z2DJprC9VtT/s8ddXAJ8MipOkQI7jhTZNor6LEVvEqNlfMedsFGvyb/rYJWlLK7GTejbxmpiUFrhgVQ3TkC/swHDt7oIivfNQ90Y4xWGmQJ93Rtk0FCrsXV0BnW2MXaR67wrT8PO/w9FMtoadvC4hZH8xz1B0mGiIDz/x+B1wu1IR/UcxTFMWcDr3Di36hikXOSfz7l/gUUxp+Rrf2XpJ5zsKmndUIu1bWMEY4VdgmOToWR5ne5Lm+YPs/VrZJa9iyPEL/rfJpR7DB2Nrtqj2CrXhraHFgMEnL0FGgAtC5SoiP6EQUdsegio8iju9f8TTA4Gk7eK1PnMu3hx"
`endif