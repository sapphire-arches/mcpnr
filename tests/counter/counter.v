module top ();
  wire [3:0] cout;
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
    .NLIGHT(4)
  ) output_lights (
    .I({cout})
  );

  test_counter dut (.CLK(clk), .RST(rst), .COUNT(cout));
endmodule

module test_counter (
  input            CLK,
  input            RST,
  output reg [3:0] COUNT,
);
  always @(posedge CLK)
    if (RST)
      COUNT <= 4'd0;
    else
      COUNT <= COUNT + 4'd1;
endmodule
