/*
* AtomicCompare testing note:
*   1. AtomicCompare size is set by awsize
*       a. size = 2^{awsize} -- awsize = 1 for 2B
*       b. This is always double the target block size (comapre + swap):
*   2. AtomicCompare swap position is set by awburst:
*       a. 2'b01, swap bytes higher than the compared bytes
*       b. 2'b10, swap bytes lower than the compared bytes
*   3. Responds:
*       a. B chan for done
*       b. R chan, orig data
*   4. awaddr[5:0] for targetd byte for compare / swap
*       a. This must be aligned to the compared size:
*           i. If compared/swap val is 2 byte, the awaddr must be 2-byte
*           aligned
*   5. wstrb assert for compared bytes, deasserted for the rest
*   6. awatop is set to 6'b110001. 
*   7. awuser, lower 4 bits config for cache wr_state is ignored
*       a. However, watch out for HDM bit
* */

/*
* Example:
*   1. Atomic CAS, for 4B byte at byte addr 0x12345600
*   2. awsize = 2'b10;
*   3. wstrb = 0xFF
*   4. awatop hard coded 
*   5. awaddr, is 6'h0 (aligned to 4B)
*   6. awburst, 2'h1 (lower byte is compared, upper is used for swap)
*   7. wdata = {56'h0, 4'h(swap), 4'h(compared)}
*
* Checking:
*   1. Set val at addr to 0xDEADBEEF
*   2. Do 4B CAS at lower bit, wdata = {56'h0, 4'h{11}, 4'h{EF}};
*   3. Expect B chan done, and ret 4'hEF in R chan
*   4. Expect host see 0xDEADBE11 at addr 
*   5. Get latency from CAS start to B chan done
*
* Control
*   CSR, send address, send data, send trigger
*   Host spin poll CSR for completion
*       i. completion in the form of inc in finished write counter
*   Upon completion, host check rdata in CSR
*   CSR needed (64 bits):
*   1. Start
*   2. Addr
*   3. Config
*   4. Data ( 64 bits? cmp swap less than 4B?), only lower 64bit is
*   considered
* */



module op_test
import CAS_types::*;
(

    input logic axi4_mm_clk,
    input logic axi4_mm_rst_n,

    input logic [63:0]      csr_op_pa, // byte level address
    input logic [511:0]      csr_op_wr_data,
    input logic [63:0]      csr_op_ret_pa,
    input logic             csr_start_op,
    output logic            csr_end_op,
    output logic [511:0]    csr_op_ret_data,
    input logic [63:0] csr_config_bits,
    output  logic [63:0]    csr_timer,


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

axi_config_bits_t axi_config_r;

logic [511:0] rdata_reg;
logic w_handshake;
logic aw_handshake;
logic [11:0] w_cnt;
logic [63:0] timer;
logic timer_enabled;
logic [63:0] output_addr_r;

(* preserve_for_debug *) wr_state_t wr_state, next_wr_state;
(* preserve_for_debug *) rd_state_t rd_state, next_rd_state;


assign  awlen        = '0   ;
assign  awprot       = '0   ;
assign  awqos        = '0   ;

assign  awcache      = '0   ;
assign  awlock       = '0   ;
assign  awregion     = '0   ;

assign  wuser        = '0   ;

assign  arlen        = '0   ;
assign  arsize       = 3'b110   ; // must tie to 3'b110
assign  arburst      = '0   ;
assign  arprot       = '0   ;
assign  arqos        = '0   ;

assign  arcache      = '0   ;
assign  arlock       = '0   ;
assign  arregion     = '0   ;

assign csr_timer = timer;
assign csr_op_ret_data = rdata_reg;



/*---------------------------------
functions
-----------------------------------*/
function void set_default();
    awvalid = 1'b0;
    wvalid = 1'b0;
    bready = 1'b0;
    arvalid = 1'b0;
    //rready = 1'b0;
    arid = 'b0;
    araddr = 'b0;
    wdata = csr_op_wr_data;
    aruser = 'b0;
    awaddr = 'b0;
    awid = 'b0;
    awuser = 'b0; 
    wlast = 1'b0;
    wstrb = 64'h0;
    awatop = '0; 
    awsize = 3'b110   ; // must tie to 3'b110 for non-atomic read writes
    awburst = '0; // set to 0 for non-atomic read writes
endfunction

function void set_signal_push();
    // 64B aligned
    awaddr = {csr_op_ret_pa[63:6], 6'b0};   
    // Non-Cacheable push
    awuser = 6'b10;
    compute_mask_general(csr_op_ret_pa[5:0], wstrb);
    wdata = rdata_reg;
    awid = w_cnt;
endfunction

function void compute_mask_general(input bit[5:0] addr, output bit[63:0] mask);
    mask = 64'hFFFF << addr;
endfunction

// 8B CAS specific mask
function void compute_cas_mask(input bit[5:0] addr, output bit[63:0] mask);
    case(addr)
        5'h0:
            mask = 64'hFFFF;
        5'h8:
            mask = 64'hFFFF00;
        5'h30, 5'h38:
            mask = 64'hFFFF000000000000;
        default:
            mask = 64'hFF; // XXX, assume addr is always lower bits???
    endcase
endfunction

// Assume 8B swap
function void set_signal_mCAS();
    // hard coded for 4B mCAS
    awsize = 3'd4;
    // From intel's example, not sure behavior for CAS at addr > 16?
    compute_cas_mask(output_addr_r[5:0], wstrb);
    // hard coded for atomic
    awatop = 6'b110001;
    // software must gaurantee 4B alignment
    awaddr = output_addr_r;
    // bit 3 -- check upper / lower is used
    awburst = output_addr_r == 5'h38 ? 2'b10 : 2'b01;
    // keep inc
    awid = w_cnt;
    // bits ignored, except for the bias / HDM bit
    awuser = axi_config_r.awuser; 
endfunction

task set_op();
    // Assume all been through CDC
    if (csr_start_op && wr_state == STATE_WR_RESET) begin
        output_addr_r <= csr_op_pa;
        axi_config_r <= axi_config_bits_t'(csr_config_bits);
    end
endtask

task set_timer();
    if (timer_enabled) begin
        timer <= timer + 'b1;
    end else if (next_wr_state == STATE_WR_SUB) begin
        timer <= 'b0; 
    end else begin
        timer <= timer;
    end
endtask

always_ff @(posedge axi4_mm_clk) begin
    if (!axi4_mm_rst_n) begin
        wr_state <= STATE_WR_RESET;
        rdata_reg <= 512'b0;
        csr_end_op <= 1'b0;
        output_addr_r <= 'b0;

        w_handshake <= 1'b0;
        aw_handshake <= 1'b0;

        w_cnt <= 'b0;
        timer <= 'b0;

        output_addr_r <= 'b0;
        axi_config_r <= axi_config_bits_t'(24'h0);
    end
    else begin
        wr_state <= next_wr_state;
        rd_state <= next_rd_state;
        set_op();
        set_timer();
        csr_end_op <= 1'b0;
        unique case(wr_state) 
            STATE_WR_SUB: begin
                if (awvalid & awready) begin
                    aw_handshake <= 1'b1;
                end
                if (wvalid & wready) begin  // write can start, otherwise wait 
                    w_handshake <= 1'b1;
                end
            end

            STATE_WR_SUB_RESP: begin
                if (bvalid & bready) begin  // write done
                    aw_handshake <= 1'b0;
                    w_handshake <= 1'b0;
                end

                if (next_wr_state != wr_state) begin
                    w_cnt <= w_cnt + 1'b1;
                end 
            end

            STATE_WR_ORIG: begin
                if (awvalid & awready) begin
                    aw_handshake <= 1'b1;
                end
                if (wvalid & wready) begin  // write can start, otherwise wait 
                    w_handshake <= 1'b1;
                end
            end

            STATE_WR_ORIG_RESP: begin
                if (bvalid & bready) begin  // write done
                    aw_handshake <= 1'b0;
                    w_handshake <= 1'b0;
                end

                if (next_wr_state != wr_state) begin
                    w_cnt <= w_cnt + 1'b1;
                end 
            end
        endcase

        unique case(rd_state)
            STATE_RD_DATA: begin
                if (rready & rvalid) begin
                    rdata_reg <= rdata;
                end
            end
            default: begin
            end
        endcase

        if (wr_state == STATE_WR_ORIG_RESP) begin
            csr_end_op <= 1'b1;
        end 
    end
end


/*---------------------------------
FSM, TODO, parallel write latency test
-----------------------------------*/

/*---------------------------------
FSM, serial read resp 
-----------------------------------*/
always_comb begin
    next_rd_state = rd_state;
    timer_enabled = 1'b0;
    rready = 1'b0;
    unique case(rd_state)
        STATE_RD_RESET: begin
            if (csr_start_op) begin
                next_rd_state = STATE_RD_DATA;
            end
        end 
        STATE_RD_DATA: begin
            timer_enabled = 1'b0;
            rready = 1'b1;
            if (rready & rvalid) begin
                next_rd_state = STATE_RD_DONE; 
            end
        end
        STATE_RD_DONE: next_rd_state = STATE_RD_RESET;
    endcase
end

/*---------------------------------
FSM, serial write latency test
-----------------------------------*/
always_comb begin
    next_wr_state = wr_state;
    unique case(wr_state)
        STATE_WR_RESET: begin
            if (csr_start_op) begin
                next_wr_state = STATE_WR_SUB;
            end else begin
                next_wr_state = STATE_WR_RESET;
            end
        end

        // NO read, always write for mCAS testing
        STATE_WR_SUB: begin
            if (awready & wready) begin
                next_wr_state = STATE_WR_SUB_RESP;
            end
            else if (wvalid == 1'b0) begin
                if (awready) begin
                    next_wr_state = STATE_WR_SUB_RESP;
                end
                else begin
                    next_wr_state = STATE_WR_SUB;
                end
            end
            else if (awvalid == 1'b0) begin
                if (wready) begin
                    next_wr_state = STATE_WR_SUB_RESP;
                end
                else begin
                    next_wr_state = STATE_WR_SUB;
                end
            end
            else begin
                next_wr_state = STATE_WR_SUB;
            end
        end

        STATE_WR_SUB_RESP: begin
            if (bvalid & bready) begin
                next_wr_state = STATE_WR_ORIG; 
            end
            else begin
                next_wr_state = STATE_WR_SUB_RESP;
            end
        end
        default: begin
            next_wr_state = STATE_WR_RESET;
        end

        STATE_WR_ORIG: begin
            if (awready & wready) begin
                next_wr_state = STATE_WR_ORIG_RESP;
            end else if (!wvalid && awready) begin
                next_wr_state = STATE_WR_ORIG_RESP;
            end else if (!awvalid && wready) begin
                next_wr_state = STATE_WR_ORIG_RESP;
            end else begin
                next_wr_state = STATE_WR_ORIG;
            end 
        end 

        STATE_WR_ORIG_RESP: begin
            if (bvalid & bready) begin
                next_wr_state = STATE_WR_RESET; 
            end
            else begin
                next_wr_state = STATE_WR_ORIG_RESP;
            end
        end 
    endcase
end

always_comb begin
    set_default();
    if (wr_state == STATE_WR_RESET || wr_state == STATE_WR_SUB) begin
        set_signal_mCAS();
    end else if (wr_state == STATE_WR_SUB_RESP || wr_state == STATE_WR_ORIG) begin
        set_signal_push();
    end 

    unique case(wr_state)
        STATE_WR_SUB: begin
            if (aw_handshake == 1'b0) begin
                awvalid = 1'b1;
            end
            else begin
                awvalid = 1'b0;
            end

            if (w_handshake == 1'b0) begin
                wvalid = 1'b1;
            end
            else begin
                wvalid = 1'b0;
            end
            wlast = 1'b1;
        end

        STATE_WR_SUB_RESP: begin
            bready = 1'b1;
        end

        STATE_WR_ORIG: begin
            if (aw_handshake == 1'b0) begin
                awvalid = 1'b1;
            end
            else begin
                awvalid = 1'b0;
            end

            if (w_handshake == 1'b0) begin
                wvalid = 1'b1;
            end
            else begin
                wvalid = 1'b0;
            end
            wlast = 1'b1;
        end

        STATE_WR_ORIG_RESP: begin
            bready = 1'b1;
        end

        default: begin
        end
    endcase
end
    

endmodule
