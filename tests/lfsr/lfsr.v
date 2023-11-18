module top ();
  wire [63:0] lout;
  wire clk, rst;

  MCPNR_SWITCHES #(
    .POS_X(0),
    .POS_Y(0),
    .POS_Z(0),
    .NSWITCH(2),
  ) input_switches (
    .O({clk, rst})
  );

  MCPNR_LIGHTS #(
    .POS_X(0),
    .POS_Y(8),
    .POS_Z(0),
    .NLIGHT(64)
  ) output_lights (
    .I({lout})
  );

  test_lfsr dut (.CLK(clk), .RST(rst), .OUTPUT(lout));
endmodule

module test_lfsr (
  input             CLK,
  input             RST,
  output reg [63:0] OUTPUT,
);
  always @(posedge CLK)
    if (RST)
      OUTPUT <= 64'd0;
    else begin
      OUTPUT[62:0] <= OUTPUT[63:1];
      OUTPUT[63] <= OUTPUT[0] ^ OUTPUT[8] ^ OUTPUT[13] ^ OUTPUT[31] ^ 1;
    end
endmodule
