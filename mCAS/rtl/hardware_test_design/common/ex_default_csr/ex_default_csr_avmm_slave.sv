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

module ex_default_csr_avmm_slave #(
    parameter REGFILE_SIZE = 32,
    parameter UPDATE_SIZE = 8
)(
 
// AVMM Slave Interface
   input               clk,
   input               reset_n,
   input  logic [63:0] writedata,
   input  logic        read,
   input  logic        write,
   input  logic [7:0]  byteenable,
   output logic [63:0] readdata,
   output logic        readdatavalid,
   input  logic [31:0] address,
   input  logic        poison,
   output logic        waitrequest,

   // monitoring
   input  logic afu_clk,
   input  logic [1:0] cxlip2iafu_read_eclk,
   input  logic [1:0] cxlip2iafu_write_eclk,

   output logic [63:0]    csr_op_pa, // byte level address
   output logic [511:0]    csr_op_wr_data, 
   output logic           csr_start_op,
   input  logic           csr_end_op,
   input  logic [63:0]   csr_op_ret_data,
   output logic [63:0]    csr_config_bits,
   input  logic [63:0]    csr_timer,

   input logic start_request,
   input logic[63:0]     target_pa,
   input logic[63:0]     compare_value,
   input logic[63:0]     swap_value,

   output logic[63:0]     match_ar_addr,
   output logic[63:0]     match_aw_addr
);

    logic [63:0] data [REGFILE_SIZE];    // CSR regfile
    logic [63:0] readdata_gray;
    logic [63:0] csr_config;
    logic [63:0] mask;
    logic [19:0] address_shift3;
    logic config_access; 
    logic [63:0]    csr_timer_aclk, csr_timer_buf;

    //Control Logic
    enum int unsigned { IDLE = 0, WRITE = 2, READ_GRAY = 3, READ_GRAY_2 = 4, READ = 5 } state, next_state;


    assign mask[7:0]   = byteenable[0]? 8'hFF:8'h0; 
    assign mask[15:8]  = byteenable[1]? 8'hFF:8'h0; 
    assign mask[23:16] = byteenable[2]? 8'hFF:8'h0; 
    assign mask[31:24] = byteenable[3]? 8'hFF:8'h0; 
    assign mask[39:32] = byteenable[4]? 8'hFF:8'h0; 
    assign mask[47:40] = byteenable[5]? 8'hFF:8'h0; 
    assign mask[55:48] = byteenable[6]? 8'hFF:8'h0; 
    assign mask[63:56] = byteenable[7]? 8'hFF:8'h0; 
    assign config_access = address[21];  
    assign address_shift3 = address[22:3];


    //Write logic
    always @(posedge clk) begin : config_write_logic
        if (!reset_n) begin
            csr_config <= '0;
            for (int i = UPDATE_SIZE; i < REGFILE_SIZE; i++) begin
                if (write && address_shift3 == i) begin
                    data[i] <= '0;
                end
            end
        end else begin
            if (write && address[20:0] == 'b0 && writedata == 64'hACE0BEEF) begin
                csr_config <= 100;
            end

            if (csr_config > 0) begin
                csr_config <= csr_config - 1;
            end 
            for (int i = UPDATE_SIZE; i < REGFILE_SIZE; i++) begin
                if (write && address_shift3 == i) begin
                    data[i] <= writedata & mask;
                end
            end

            if (start_request) begin
                data[24] <= target_pa;
                data[25] <= compare_value;
                data[26] <= swap_value;
            end
            data[27] <= csr_op_ret_data;
        end    
    end 

    //Read logic
    always @(posedge clk) begin
        if (!reset_n) begin
            readdata  <= 64'h0;
        end
        else begin
            readdata <= readdata_gray;    
            if (read && (address_shift3 < REGFILE_SIZE) && (state == IDLE)) begin 
                readdata_gray <= data[address_shift3] & mask; // Use synchronizer
            end else if(read && (address[20:0] == '0) && config_access && (state == IDLE)) begin
                readdata_gray <= csr_config & mask;
            end else begin
                readdata_gray <= {32'hFEDCBA00, address[15:0], 16'hABCD};
            end    
        end    
    end 


    always_comb begin : next_state_logic
        next_state = IDLE;
            case(state)
            IDLE    : begin 
                if( write ) begin
                    next_state = WRITE;
                end else if (read) begin
                    next_state = READ_GRAY;
                end else begin
                    next_state = IDLE;
                end
            end

            WRITE     : begin
                next_state = IDLE;
            end

            READ_GRAY : begin
                next_state = READ;
            end

            READ_GRAY_2: begin
                next_state = READ;
            end

            READ      : begin
                next_state = IDLE;
            end

            default : next_state = IDLE;
        endcase
    end


    always_comb begin
    case(state)
        IDLE    : begin
            waitrequest  = 1'b1;
            readdatavalid= 1'b0;
        end
        WRITE     : begin 
            waitrequest  = 1'b0;
            readdatavalid= 1'b0;
        end
        READ_GRAY, READ_GRAY_2: begin
            waitrequest  = 1'b1;
            readdatavalid= 1'b0;
        end
        READ     : begin 
            waitrequest  = 1'b0;
            readdatavalid= 1'b1;
        end
        default : begin 
            waitrequest  = 1'b1;
            readdatavalid= 1'b0;
        end
    endcase
    end

    always_ff@(posedge clk) begin
        if(~reset_n) begin
            state <= IDLE;
        end else begin
            state <= next_state;
        end
    end


    // ==============================
    // input --> register 
    // ==============================
    logic read_flag;
    logic write_flag;
    logic [63:0] debug_counter;
    logic [63:0] memRead_counter;
    logic [63:0] memWrite_counter;
    logic [63:0] memRead_counter_buf;
    logic [63:0] memWrite_counter_buf;
    logic [63:0] memRead_counter_aclk;
    logic [63:0] memWrite_counter_aclk;
    logic [5:0]  sync_cnt;

    // CDC for read write counters
    always_ff @( posedge afu_clk ) begin
        if (~reset_n) begin
            sync_cnt            <= '0;
            memRead_counter     <= '0;
            memWrite_counter    <= '0;
        end else begin
            if (cxlip2iafu_read_eclk[1] & cxlip2iafu_read_eclk[0] ) begin
                memRead_counter <= memRead_counter + 2;
            end else if (cxlip2iafu_read_eclk[1] ^ cxlip2iafu_read_eclk[0] ) begin
                memRead_counter <= memRead_counter + 1;
            end

            if (cxlip2iafu_write_eclk[1] & cxlip2iafu_write_eclk[0] ) begin
                memWrite_counter <= memWrite_counter + 2;
            end else if (cxlip2iafu_write_eclk[1] ^ cxlip2iafu_write_eclk[0] ) begin
                memWrite_counter <= memWrite_counter + 1;
            end
            // Assign the counter to the counter buffer every 2^6 cycles
            sync_cnt <= sync_cnt + 1'b1;
            if (sync_cnt == 0) begin
                memRead_counter_buf     <= memRead_counter;
                memWrite_counter_buf    <= memWrite_counter;
                csr_timer_buf           <= csr_timer;
            end
        end
    end

    always_ff @( posedge clk ) begin
        // A naive two-stage synchronizer
        memRead_counter_aclk    <= memRead_counter_buf;
        memWrite_counter_aclk   <= memWrite_counter_buf;
        csr_timer_aclk          <= csr_timer_buf;
    end

    task reset_reg();
        read_flag           <= 1'b0;
        write_flag          <= 1'b0;
        debug_counter       <= '0;
        for (int i = 0; i < UPDATE_SIZE; i++) begin
            data[i] <= '0;
        end
    endtask

    task set_reg_0();
        // clock
        debug_counter <= debug_counter + 1;
        if (debug_counter >= 10000) begin
            data[0]         <= data[0] + 1;
            debug_counter   <= '0;
        end
    endtask

    task set_reg_1();
        data[1] <= memRead_counter_aclk;
    endtask

    task set_reg_2();
        data[2] <= memWrite_counter_aclk;
    endtask

    task set_reg_3();
        if (start_request) data[3] <= data[3] + 64'h1;
    endtask

    always_ff @( posedge clk ) begin : m5_monitor_logic
        if (!reset_n) begin
            reset_reg();
        end else begin 

            set_reg_0();

            set_reg_1();

            set_reg_2();

            set_reg_3();

            data[7] <= csr_timer_aclk;
        end
    end

    // ==============================
    // register --> output
    // ==============================
    always_comb begin
        // Triggers 8, 9, 10
        csr_start_op = 1'b0;
        case(address_shift3) 
            'd8: begin
            csr_start_op = write & ((writedata & mask) != 0);
            end
            default: begin
            end
        endcase

        // Data 11 - 23
        csr_op_pa = data[11];
        csr_config_bits = data[12];
        match_ar_addr = data[13];
        match_aw_addr = data[14];
        csr_op_wr_data = '0;

        // reg_24-31 used by prefetech data debug
       
    end
endmodule
