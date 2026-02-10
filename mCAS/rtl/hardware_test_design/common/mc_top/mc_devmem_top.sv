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


module mc_devmem_top 
import mc_ecc_pkg::*; 
(
  input  logic                                    clk,
  input  logic                                    rst,

  input   mc_ecc_pkg::mc_devmem_if_t              mc_devmem_if,
  output  mc_ecc_pkg::mc_err_cnt_t                mc_err_cnt

    );


  logic [32:0]  mcRdDataDBECnt_Q;
  logic [3:0]   mcRdDataDBESum;
  logic         mcRdDataNewDBE_Q;
  logic         mcRdDataNewPoisonRtn_Q;
  logic         mcRdDataNewSBE_Q;
  logic [32:0]  mcRdDataPoisonRtnCnt_Q;
  logic [32:0]  mcRdDataSBECnt_Q;
  logic [3:0]   mcRdDataSBESum;
  logic         mcErrOnPartial_Q;

  // Generate sum of SBE[7:0] and DBE[7:0]
  always_comb begin
    mcRdDataSBESum = '0;
    mcRdDataDBESum = '0;

    for (int i=0; i<8; i++) begin
      mcRdDataSBESum += mc_devmem_if.RdDataECC.SBE[i];
      mcRdDataDBESum += mc_devmem_if.RdDataECC.DBE[i];
    end
  end

  always_ff @(posedge clk) begin
    if (rst) begin
      mcRdDataSBECnt_Q <= '0;
      mcRdDataNewSBE_Q <= 1'b0;
    end
    else if (mc_devmem_if.RdDataECC.Valid & (|mc_devmem_if.RdDataECC.SBE)) begin
      mcRdDataSBECnt_Q <= mcRdDataSBECnt_Q + mcRdDataSBESum;
      mcRdDataNewSBE_Q <= 1'b1;  // Update channel aggregated count
    end
    else if (mcRdDataNewSBE_Q) begin
      mcRdDataNewSBE_Q <= 1'b0;
    end
  end

  // - If all DBE instances report error, treat as data written to memory with poison=1
  //   - Not treated as a DBE error case
  //     - Increment "poison return" count
  //     - Do not increment DBE count
  //   - Note: Read data returned to host will have poison=1 (any DBE causes poison=1)
  // - If some (not all) DBE instances report error, treat as data written to memory with poison=0
  //   - Treated as a DBE error case
  //     - Increment DBE count
  //     - Increment "poison return" count
  //   - Note: Read data returned to host will have poison=1 (any DBE causes poison=1)

  always_ff @(posedge clk) begin
    if (rst) begin
      mcRdDataPoisonRtnCnt_Q <= '0;
      mcRdDataNewPoisonRtn_Q <= 1'b0;
    end
    else if (mc_devmem_if.RdDataValid & (|mc_devmem_if.RdDataECC.DBE)) begin
      mcRdDataPoisonRtnCnt_Q <= mcRdDataPoisonRtnCnt_Q + 'd1;
      mcRdDataNewPoisonRtn_Q <= 1'b1;  // Update channel aggregated count
    end
    else if (mcRdDataNewPoisonRtn_Q) begin
      mcRdDataNewPoisonRtn_Q <= 1'b0;
    end
  end

  always_ff @(posedge clk) begin
    if (rst) begin
      mcRdDataDBECnt_Q <= '0;
      mcRdDataNewDBE_Q <= 1'b0;
    end
    else if (mc_devmem_if.RdDataECC.Valid & (|mc_devmem_if.RdDataECC.DBE) & (~&mc_devmem_if.RdDataECC.DBE)) begin
      mcRdDataDBECnt_Q <= mcRdDataDBECnt_Q + mcRdDataDBESum;
      mcRdDataNewDBE_Q <= 1'b1;  // Update channel aggregated count
    end
    else if (mcRdDataNewDBE_Q) begin
      mcRdDataNewDBE_Q <= 1'b0;
    end
  end

  // Create indicator for mbox logic to know if Err indicator is for partial write
  //  From mc_channel_adapter:
  //      If both mc2iafu_readdatavalid_eclk == 1 and mc2iafu_ecc_err_valid_eclk == 1
  //        then *ecc_err_* are related to mc2iafu_readdata_eclk
  //      If mc2iafu_readdatavalid_eclk == 0 and mc2iafu_ecc_err_valid_eclk == 1
  //        then *ecc_err_* are related to partial write. "Partial write" functionality is realised as read-modify-write function.

  always_ff @(posedge clk) begin
    if (rst) begin
      mcErrOnPartial_Q <= '0;
    end
    else if (mc_devmem_if.RdDataECC.Valid & ~mc_devmem_if.RdDataValid) begin
      mcErrOnPartial_Q <= '1;
    end
    else begin
      mcErrOnPartial_Q <= '0;
    end
  end

  assign mc_err_cnt.SBECnt       = mcRdDataSBECnt_Q;
  assign mc_err_cnt.DBECnt       = mcRdDataDBECnt_Q;
  assign mc_err_cnt.PoisonRtnCnt = mcRdDataPoisonRtnCnt_Q;

  assign mc_err_cnt.NewSBE       = mcRdDataNewSBE_Q;
  assign mc_err_cnt.NewDBE       = mcRdDataNewDBE_Q;
  assign mc_err_cnt.NewPoisonRtn = mcRdDataNewPoisonRtn_Q;
  assign mc_err_cnt.NewPartialWr = mcErrOnPartial_Q;



endmodule
                 
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwCgs1wO39ytN6HjqSWUILSuO2RxcOPo3cn1PnGAD7YEtHx6B+Y4XMzBbCEbmN89gCle50WH93z0cluNleMELtB5HtW/eN64aCriHvj3tglITOgRNxhq1jwqS74PARn6CMqayOpIdQ1pbvFUDzvCVpv1hTSUyetYSJF0vWX/2ebpUTq6B+8zwztOMdvc9YqBQMLdJyyXInsRdZBjeB1aHDC5nGXd7WpsU5sataayMllqceNK5D7bUhSOb/5pwiWIpZptlcF0lxCSjUx0ij1rzjOfHUh2a+fAoGnxX8wSqIbWT1hOGFpXVGQ2FDEHuWWyHowSI2CbqCDklTX7NTDcZCsBLABAjSTIHVQKTJ3FZV1cphy7t+fsYlB4a07SA418Uq11dZB1omZDJBzYtpQX28fqFce+CxEMaxcvnifGO42cU2SURHaWyp2C8bchEvl0xMEIKqNxC5o6rLED/QR2b8ZU26UShS1lnx0kyhcv5eRG0kdcy2pC6Nj1LoQVdaWzI7Ty7Ggr6YYKLOHDDuiYmDytqySwMOjA0o9Ob71F49AHOaFAqadarQ2w5dKYmVRmv6uJRsD46hp/8Ydc/G0ZxsERaJNRinWDC8wg1ITcyYNqCh3/O1MxpcLVigm0j531RhrjH8LRgKM2i6xTjPTH+ue+F2dZb/4R/pZQ32bqeb2pkdLvBJnsdQYOirDRS6bwdNnm/Va0Iq1LXDWhCanCtaKjBKE/Zzpzf+0CSq9E7ZqRyO7m55k1hmIGn3w2g6s6sBei9n35xX7c75fPEjWCvNAW"
`endif