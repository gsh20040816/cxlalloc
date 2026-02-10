typedef struct packed {
    bit[63:0] compare_value;
    bit[63:0] swap_value;
    bit[63:0] target_pa;
    bit[7:0] status;
} mCAS_strcut_t;

module mcas_ctrl

import ed_cxlip_top_pkg::*;
import ed_mc_axi_if_pkg::*;
(

    input  logic                                             afu_clk,
    input  logic                                             afu_rstn,

    // April 2023 - Supporting out of order responses with AXI4
    input  ed_mc_axi_if_pkg::t_to_mc_axi4    [MC_CHANNEL-1:0] cxlip2iafu_to_mc_axi4,
    output ed_mc_axi_if_pkg::t_to_mc_axi4    [MC_CHANNEL-1:0] iafu2mc_to_mc_axi4 ,

    input  ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] mc2iafu_from_mc_axi4,
    output ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4,

    input logic c0_ar_match, 
    input logic c0_aw_match, 

    input logic[511:0] c0_wd_data,

    input logic[5:0] c0_core_id

);
    logic           start_request;
    logic[63:0]     target_pa;
    logic[63:0]     compare_value;
    logic[63:0]     swap_value;

    ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4_stg0_r;
    ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4_stg1_r;
    ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4_stg2_r;
    ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4_stg3_r;
    ed_mc_axi_if_pkg::t_from_mc_axi4  [MC_CHANNEL-1:0] iafu2cxlip_from_mc_axi4_stg3_n;

    logic ignore_c0_wr_resp;
    logic c0_aw_match_r;
    logic[511:0] c0_wd_data_r;
    (* preserve_for_debug *) logic[7:0] w_core_id;
    assign w_core_id = c0_wd_data_r[199:192];

    logic pending_insert_r, clear_pending;
    logic[127:0] wr_data_r;
    logic[63:0] pending_target_pa;
    logic[63:0] pending_wdata;
    assign pending_target_pa = wr_data_r[127:64];
    assign pending_wdata = wr_data_r[63:0];
 
    /*
    * 63 - 0: cmp data
    * 127 - 64: swp data
    * 191 - 128: CXL PA for CAS
    * */
    localparam MAX_CORE_CNT = 64;
    mCAS_strcut_t struct_arr_r[MAX_CORE_CNT];
    mCAS_strcut_t struct_arr_n[MAX_CORE_CNT];


    // directly connected signals 
    assign iafu2cxlip_from_mc_axi4[1] = mc2iafu_from_mc_axi4[1];

    assign iafu2cxlip_from_mc_axi4[0].awready = mc2iafu_from_mc_axi4[0].awready & ~pending_insert_r;
    assign iafu2cxlip_from_mc_axi4[0].wready = mc2iafu_from_mc_axi4[0].wready & ~pending_insert_r;
    assign iafu2cxlip_from_mc_axi4[0].arready = mc2iafu_from_mc_axi4[0].arready & ~pending_insert_r;

    assign iafu2cxlip_from_mc_axi4[0].bvalid = mc2iafu_from_mc_axi4[0].bvalid & ~ignore_c0_wr_resp;
    assign iafu2cxlip_from_mc_axi4[0].bid = mc2iafu_from_mc_axi4[0].bid;
    //assign iafu2cxlip_from_mc_axi4[0].buser = mc2iafu_from_mc_axi4[0].buser;
    assign iafu2cxlip_from_mc_axi4[0].buser = 'b0;
    assign iafu2cxlip_from_mc_axi4[0].bresp = mc2iafu_from_mc_axi4[0].bresp;

    // delayed signals 
    assign iafu2cxlip_from_mc_axi4[0].rvalid = iafu2cxlip_from_mc_axi4_stg3_r[0].rvalid;
    assign iafu2cxlip_from_mc_axi4[0].rlast = iafu2cxlip_from_mc_axi4_stg3_r[0].rlast;
    assign iafu2cxlip_from_mc_axi4[0].rid = iafu2cxlip_from_mc_axi4_stg3_r[0].rid;
    assign iafu2cxlip_from_mc_axi4[0].rdata = iafu2cxlip_from_mc_axi4_stg3_r[0].rdata;
    assign iafu2cxlip_from_mc_axi4[0].ruser = iafu2cxlip_from_mc_axi4_stg3_r[0].ruser;
    assign iafu2cxlip_from_mc_axi4[0].rresp = iafu2cxlip_from_mc_axi4_stg3_r[0].rresp;

    (* preserve_for_debug *) logic[7:0] req_set_data, rsp_set_data, rsp_load_data_w, rsp_load_data_r;
    (* preserve_for_debug *) logic[7:0] req_set_addr, rsp_set_addr, rsp_load_addr;
    logic req_set, rsp_set;
    logic clear_status_n;
    logic[5:0] clear_entry_n;

    (* preserve_for_debug *) logic debug_failed_inc;
    assign debug_failed_inc = ((compare_value + 64'h1) != swap_value) & start_request;

    (* preserve_for_debug *) logic debug_miss_expected;
    (* preserve_for_debug *) logic[63:0] prev_swap_value_r;
    assign debug_miss_expected = ((prev_swap_value_r + 64'h1) != swap_value) & start_request;

    (* preserve_for_debug *) logic same_core_req_both_ch;
    assign same_core_req_both_ch = (c0_ar_match && c0_aw_match_r && (c0_core_id == w_core_id));



    function void set_default();
        start_request = 1'b0;
        target_pa = '0;
        compare_value = '0;
        swap_value = '0;
    endfunction

    /*
    * ready_tracking
    * 0 0
    */
    function void set_rd_matching();
        req_set_addr = '0;
        req_set_data = '0;
        req_set = 1'b0;
        if (c0_ar_match) begin
            if (struct_arr_r[c0_core_id].status == 8'b1) begin
                iafu2mc_to_mc_axi4[0].araddr = struct_arr_r[c0_core_id].target_pa[51:0];
                struct_arr_n[c0_core_id].status = 8'b10;
                req_set_addr = cxlip2iafu_to_mc_axi4[0].arid;
                req_set = 1'b1;
                req_set_data = {2'b11, c0_core_id};
            end
        end
    endfunction

    function void set_wr_matching();
        if (c0_aw_match_r) begin
            struct_arr_n[w_core_id].compare_value = c0_wd_data_r[63:0];
            struct_arr_n[w_core_id].target_pa = c0_wd_data_r[191:128];
            struct_arr_n[w_core_id].swap_value = c0_wd_data_r[127:64];
            struct_arr_n[w_core_id].status = 8'b1;
        end
    endfunction

    logic issue_write;
    assign issue_write = pending_insert_r && mc2iafu_from_mc_axi4[0].wready;

    function void insert_wr();
        clear_pending = 1'b0;
        // idle aw channel
        if (issue_write) begin
            // 8 bit
            // iafu2mc_to_mc_axi4[0].awid = cxlip2iafu_to_mc_axi4[0].awid[7:0];
            iafu2mc_to_mc_axi4[0].awid = 8'hFF;
            iafu2mc_to_mc_axi4[0].awuser = 6'h1;
            iafu2mc_to_mc_axi4[0].awvalid = 1'b1;
            iafu2mc_to_mc_axi4[0].awaddr = pending_target_pa[51:0];
        end
        // same cycle
        if (issue_write) begin
            iafu2mc_to_mc_axi4[0].wdata = {448'b0, pending_wdata};
            iafu2mc_to_mc_axi4[0].wvalid = 1'b1;
            iafu2mc_to_mc_axi4[0].wlast = 1'b1;
            iafu2mc_to_mc_axi4[0].wstrb = 64'hffffffffffffffff;

            // XXX should be the same as aw channel 
            clear_pending = 1'b1;
        end
        iafu2mc_to_mc_axi4[0].arvalid = cxlip2iafu_to_mc_axi4[0].arvalid & ~pending_insert_r;
    endfunction

    /*
    * For a rid resp, check if we send out a mCAS
    */
    /*
    * mc2iafu_from_mc_axi4 (rsp_load_addr)
    *   stg0_r
    *       stg1_r (rsp_load_data_w)
    *           stg2_r (resp_struct_r, rsp_load_data_r)
    *               stg3_r (iafu2mc_to_mc_axi4)
    *                   
    */

    (* preserve_for_debug *) mCAS_strcut_t resp_struct_r, resp_struct_n;
    function void set_rep_routing();
        iafu2cxlip_from_mc_axi4_stg3_n = iafu2cxlip_from_mc_axi4_stg2_r;
        rsp_set_addr = '0;
        rsp_set_data = '0;
        rsp_set = 1'b0;
        clear_status_n = 1'b0;
        clear_entry_n = 6'b0;
        resp_struct_n = resp_struct_r;

        // Upon resp, check the rid, data ret in next cycle
        rsp_load_addr = mc2iafu_from_mc_axi4[0].rid;

        // if table ret data 
        if (iafu2cxlip_from_mc_axi4_stg1_r[0].rvalid && rsp_load_data_w[7]) begin
            resp_struct_n = struct_arr_r[rsp_load_data_w[5:0]];

            // immediately clear the status bit
            clear_status_n = 1'b1;
            clear_entry_n = rsp_load_data_w[5:0];

            // reset the entry
            rsp_set_addr = iafu2cxlip_from_mc_axi4_stg1_r[0].rid;
            rsp_set_data = '0;
            rsp_set = 1'b1;

        end else begin
            // otherwise, make sure not to compare in the next cycle/stg
            // (stg2_r)
            resp_struct_n.status = 8'b0;
        end

        if (iafu2cxlip_from_mc_axi4_stg2_r[0].rvalid && rsp_load_data_r[7]) begin
            // default to cmp failed
            iafu2cxlip_from_mc_axi4_stg3_n[0].rdata[127:64] = 64'h0;
            //if matches the value for the slot
            // if slot is mCAS
            if (resp_struct_r.status == 8'b10) begin
                // compare
                if ((iafu2cxlip_from_mc_axi4_stg2_r[0].rdata[63:0] == resp_struct_r.compare_value)) begin

                    // match, return succeed signal for read resp
                    //  lower 64 bit is already set by the read resp (as
                    //  original value)
                    iafu2cxlip_from_mc_axi4_stg3_n[0].rdata[127:64] = 64'h1;

                    // match, issue swap value write
                    start_request = 1'b1;
                    target_pa = resp_struct_r.target_pa;
                    swap_value = resp_struct_r.swap_value;
                    compare_value = resp_struct_r.compare_value;

                    // XXX, write may not fully propogated until the next mCAS

                    // bully 
                    for (int i = 0; i < MAX_CORE_CNT; i++) begin

                        // if target_pa matches, then current one has the most
                        // authority
                        if (struct_arr_r[i].target_pa == target_pa) begin
                            // clear the other status
                            struct_arr_n[i].status = 8'b0;
                        end
                        if (resp_struct_n.target_pa == target_pa) begin
                            resp_struct_n.status = 8'b0;
                        end
                    end
                end
            end
        end
    endfunction

    // ignore tying responses to requests made by mcas
    function void track_wr_resp();
        ignore_c0_wr_resp = 1'b0;
        if (mc2iafu_from_mc_axi4[0].buser == 1'b1 &&
            mc2iafu_from_mc_axi4[0].bvalid && iafu2mc_to_mc_axi4[0].bready) begin
            ignore_c0_wr_resp = 1'b1;
        end
    endfunction

    always_comb begin
        set_default();
        iafu2mc_to_mc_axi4      = cxlip2iafu_to_mc_axi4;
        struct_arr_n = struct_arr_r;
        set_wr_matching();
        set_rd_matching();
        if (clear_status_n) begin
            struct_arr_n[clear_entry_n].status = 8'b0;
        end
        set_rep_routing();
        insert_wr();
        track_wr_resp();
    end

    always_ff @(posedge afu_clk) begin
        if (!afu_rstn) begin
            iafu2cxlip_from_mc_axi4_stg0_r <= mc2iafu_from_mc_axi4; 
            iafu2cxlip_from_mc_axi4_stg1_r <= mc2iafu_from_mc_axi4; 
            iafu2cxlip_from_mc_axi4_stg2_r <= mc2iafu_from_mc_axi4; 
            c0_aw_match_r <= '0;
            c0_wd_data_r <= '0;
            pending_insert_r <= 1'b0;
            resp_struct_r <= '0;
            rsp_load_data_r <= '0;
            wr_data_r <= '0;
            prev_swap_value_r <= '0;
            struct_arr_r <= '{default: '0};

        end else begin
            iafu2cxlip_from_mc_axi4_stg0_r <= mc2iafu_from_mc_axi4; 
            iafu2cxlip_from_mc_axi4_stg1_r <= iafu2cxlip_from_mc_axi4_stg0_r;
            iafu2cxlip_from_mc_axi4_stg2_r <= iafu2cxlip_from_mc_axi4_stg1_r;
            iafu2cxlip_from_mc_axi4_stg3_r <= iafu2cxlip_from_mc_axi4_stg3_n;
            resp_struct_r <= resp_struct_n;
            c0_aw_match_r <= c0_aw_match;
            c0_wd_data_r <= c0_wd_data;
            rsp_load_data_r <= rsp_load_data_w;
            wr_data_r <= wr_data_r;
            struct_arr_r <= struct_arr_n;
            // fetch
            if (start_request) begin
                pending_insert_r <= 1'b1;
                prev_swap_value_r <= swap_value;
                wr_data_r <= {target_pa, swap_value};
            end
            //clear
            else if (clear_pending) begin
                pending_insert_r <= 1'b0;
            end
        end
    end

    bram_4port_8w_256d table_inst(
      .clock           (afu_clk),
      .data_a          (req_set_data),          //   input,  width = 8,          data_a.datain_a
      .q_a             (rsp_load_data_w),             //  output,  width = 8,             q_a.dataout_a
      .data_b          (rsp_set_data),          //   input,  width = 8,          data_b.datain_b
      .q_b             (),             //  output,  width = 8,             q_b.dataout_b
      .write_address_a (req_set_addr), //   input,  width = 8, write_address_a.write_address_a
      .write_address_b (rsp_set_addr), //   input,  width = 8, write_address_b.write_address_b
      .read_address_a  (rsp_load_addr),  //   input,  width = 8,  read_address_a.read_address_a
      .read_address_b  (8'h0),  //   input,  width = 8,  read_address_b.read_address_b
      .wren_a          (req_set),          //   input,  width = 1,          wren_a.wren_a
      .wren_b          (rsp_set)          //   input,  width = 1,          wren_b.wren_b
    );

    logic [63:0] debug_bram_rdata;
    simple_bram debug_bram_0 (
        .afu_clk(afu_clk),
        .wport1_addr(pending_target_pa[7:3]),
        .wport1_wen(issue_write),
        .wport1_din(pending_wdata),
        .rport1_addr(pending_target_pa[7:3]),
        .rport1_dout(debug_bram_rdata)
    );
    (* preserve_for_debug *) logic debug_bram_rdata_inc_fail;
    assign debug_bram_rdata_inc_fail = ((debug_bram_rdata + 64'h1) != pending_wdata) & issue_write;

    (* preserve_for_debug *) logic [31:0] num_debug_writes_0;
    (* preserve_for_debug *) logic [31:0] num_debug_writes_1;

    always_ff @(posedge afu_clk) begin
        if (!afu_rstn) begin
            num_debug_writes_0 <= 32'h0;
            num_debug_writes_1 <= 32'h0;
        end else begin
            if (issue_write) begin
                if (pending_target_pa[7])
                    num_debug_writes_1 <= num_debug_writes_1 + 32'h1;
                else begin
                    num_debug_writes_0 <= num_debug_writes_0 + 32'h1;
                end
            end
        end    
    end

    (* preserve_for_debug *) logic bad_edge_case_0, bad_edge_case_1;
    assign bad_edge_case_0 = start_request & clear_pending;
    assign bad_edge_case_1 = start_request & pending_insert_r;

endmodule
