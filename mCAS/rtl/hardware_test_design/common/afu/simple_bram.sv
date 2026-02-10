module simple_bram( // 32 depth, 64 wide
    input logic afu_clk,
    input logic [4:0] wport1_addr,
    input logic wport1_wen,
    input logic [63:0] wport1_din,

    input logic [4:0] rport1_addr,
    output logic [63:0] rport1_dout
);

    logic [63:0] bram_mem [32];
    always_ff @(posedge afu_clk) begin
        if (wport1_wen) begin
            bram_mem[wport1_addr] <= wport1_din;
        end
    end

    assign rport1_dout = bram_mem[rport1_addr];

endmodule
