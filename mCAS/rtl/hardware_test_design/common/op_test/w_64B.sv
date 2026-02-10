/*
Module: request_read_write
Version: 0.0
Last Modified: March 5, 2024
Description: Modified based on packet generator 0.0.1
Workflow: 
    1. set request_page_addr to cxl mem address
    2. trigger start_request, nc-read + nc-p-write
    3. wait for end_request, finish
*/

module w_64B_m (

    input logic axi4_mm_clk,
    input logic axi4_mm_rst_n,

    // control logic 
    // set physical address of target cache line to request_page_addr
    input logic [63:0] target_pa, // byte level address
    input logic start_request,
    output logic end_request,
    input logic [511:0] write_value,

    // read address channel
    output logic [11:0]               arid,
    output logic [63:0]               araddr,
    output logic [9:0]                arlen,    // must tie to 10'd0
    output logic [2:0]                arsize,   // must tie to 3'b110
    output logic [1:0]                arburst,  // must tie to 2'b00
    output logic [2:0]                arprot,   // must tie to 3'b000
    output logic [3:0]                arqos,    // must tie to 4'b0000
    output logic [5:0]                aruser,   // 4'b0000": non-cacheable, 4'b0001: cacheable shared, 4'b0010: cachebale owned
    output logic                      arvalid,
    output logic [3:0]                arcache,  // must tie to 4'b0000
    output logic [1:0]                arlock,   // must tie to 2'b00
    output logic [3:0]                arregion, // must tie to 4'b0000
    input                             arready,

    // read response channel
    input [11:0]                      rid,    // no use
    input [511:0]                     rdata,  
    input [1:0]                       rresp,  // no use: 2'b00: OKAY, 2'b01: EXOKAY, 2'b10: SLVERR
    input                             rlast,  // no use
    input                             ruser,  // no use
    input                             rvalid,
    output logic                      rready,


    // write address channel
    output logic [11:0]               awid,
    output logic [63:0]               awaddr, 
    output logic [9:0]                awlen,    // must tie to 10'd0
    output logic [2:0]                awsize,   // must tie to 3'b110 (64B/T)
    output logic [1:0]                awburst,  // must tie to 2'b00
    output logic [2:0]                awprot,   // must tie to 3'b000
    output logic [3:0]                awqos,    // must tie to 4'b0000
    output logic [5:0]                awuser,
    output logic                      awvalid,
    output logic [3:0]                awcache,  // must tie to 4'b0000
    output logic [1:0]                awlock,   // must tie to 2'b00
    output logic [3:0]                awregion, // must tie to 4'b0000
    output logic [5:0]                awatop,   // must tie to 6'b000000
    input                             awready,

    // write data channel
    output logic [511:0]              wdata,
    output logic [(512/8)-1:0]        wstrb,
    output logic                      wlast,
    output logic                      wuser,  // must tie to 1'b0
    output logic                      wvalid,
    input                             wready,

    // write response channel
    input [11:0]                      bid,    // no use
    input [1:0]                       bresp,  // no use: 2'b00: OKAY, 2'b01: EXOKAY, 2'b10: SLVERR
    input [3:0]                       buser,  // must tie to 4'b0000
    input                             bvalid,
    output logic                      bready
);



assign  awlen        = '0   ;
assign  awsize       = 3'b110   ; // must tie to 3'b110
assign  awburst      = '0   ;
assign  awprot       = '0   ;
assign  awqos        = '0   ;

assign  awcache      = '0   ;
assign  awlock       = '0   ;
assign  awregion     = '0   ;
assign  awatop       = '0   ; 

assign  wuser        = '0   ;

assign  arlen        = '0   ;
assign  arsize       = 3'b110   ; // must tie to 3'b110
assign  arburst      = '0   ;
assign  arprot       = '0   ;
assign  arqos        = '0   ;

assign  arcache      = '0   ;
assign  arlock       = '0   ;
assign  arregion     = '0   ;


logic [63:0] target_pa_r;
logic [511:0] write_value_r;
logic w_handshake;
logic aw_handshake;



enum logic [4:0] {
    STATE_RESET,
    STATE_WR_SUB,
    STATE_WR_SUB_RESP
} state, next_state;

logic fsm_fetch, fifo_full, fifo_empty;
logic[575:0] fifo_out;
assign fifo_out = 576'd0;
assign fifo_full = 0;
assign fifo_empty = 0;

/*---------------------------------
functions
-----------------------------------*/
function void set_default();
    awvalid = 1'b0;
    wvalid = 1'b0;
    bready = 1'b0;
    arvalid = 1'b0;
    rready = 1'b0;
    arid = 'b0;
    araddr = 'b0;
    wdata = fifo_out[511:0];  
    aruser = 'b0;
    awaddr = 'b0;
    awid = 'b0;
    awuser = 'b0; 
    wlast = 1'b0;
    wstrb = 64'h0;
endfunction


// queue one request for TOP-2 tracker
task queue_request();
    // upon an valid issue 
    if (state == STATE_RESET) begin
        // put the proper address into "target_pa_r"
        if (start_request) begin
            target_pa_r <= target_pa;
            write_value_r <= write_value;
        end
    end
endtask

always_ff @(posedge axi4_mm_clk) begin
    if (!axi4_mm_rst_n) begin
        state <= STATE_RESET;
        end_request <= 1'b0;
        target_pa_r <= 'b0;
        write_value_r <= '0;

        w_handshake <= 1'b0;
        aw_handshake <= 1'b0;
    end
    else begin
        queue_request();
        state <= next_state;
        unique case(state) 
            STATE_WR_SUB: begin
                if (awvalid & awready) begin
                    aw_handshake <= 1'b1;
                end
                if (wvalid & wready) begin  // nc-p-write can start, otherwise wait 
                    w_handshake <= 1'b1;
                end
            end

            STATE_WR_SUB_RESP: begin
                if (bvalid & bready) begin  // nc-p-write done
                    aw_handshake <= 1'b0;
                    w_handshake <= 1'b0;
                    end_request <= 1'b1;
                end
            end

            default: begin
                end_request <= 1'b0;
            end
        endcase
    end
end




/*---------------------------------
FSM
-----------------------------------*/

always_comb begin
    next_state = state;
    fsm_fetch = 'b0;
    unique case(state)
        STATE_RESET: begin
            // guard for requesting wrong address
            if (~fifo_empty) begin
                next_state = STATE_WR_SUB;
                fsm_fetch = 1'b1;
            end else begin
                next_state = STATE_RESET;
            end
        end

        STATE_WR_SUB: begin
            if (awready & wready) begin
                next_state = STATE_WR_SUB_RESP;
            end
            else if (wvalid == 1'b0) begin
                if (awready) begin
                    next_state = STATE_WR_SUB_RESP;
                end
                else begin
                    next_state = STATE_WR_SUB;
                end
            end
            else if (awvalid == 1'b0) begin
                if (wready) begin
                    next_state = STATE_WR_SUB_RESP;
                end
                else begin
                    next_state = STATE_WR_SUB;
                end
            end
            else begin
                next_state = STATE_WR_SUB;
            end
        end

        STATE_WR_SUB_RESP: begin
            if (bvalid & bready) begin
                next_state = STATE_RESET; 
            end
            else begin
                next_state = STATE_WR_SUB_RESP;
            end
        end

        default: begin
            next_state = STATE_RESET;
        end
    endcase
end

always_comb begin
    set_default();
    unique case(state)
        STATE_WR_SUB: begin
            if (aw_handshake == 1'b0) begin
                awvalid = 1'b1;
            end
            else begin
                awvalid = 1'b0;
            end
            awid = 12'd2;
            awuser = 6'b110000; // nc-write, device-biased, HDM
            awaddr = fifo_out[575:512];

            if (w_handshake == 1'b0) begin
                wvalid = 1'b1;
            end
            else begin
                wvalid = 1'b0;
            end
            wlast = 1'b1;
            wstrb = 64'hffffffffffffffff;
        end

        STATE_WR_SUB_RESP: begin
            bready = 1'b1;
        end

        default: begin

        end
    endcase
end
    

endmodule
