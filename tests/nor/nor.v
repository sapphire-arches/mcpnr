`default_nettype none

module top (
);
  wire a, b, y;

  MCPNR_SWITCHES #(
    .POS_X(0),
    .POS_Y(0),
    .POS_Z(0),
    .NSWITCH(2),
  ) input_switches (
    .O({a, b}),
  );

  MCPNR_LIGHTS #(
    .POS_X(0),
    .POS_Y(0),
    .POS_Z(6),
  ) output_lights (
    .I({y})
  );

  test_nor dut (
    .A(a),
    .B(b),
    .Y(y),
  );
endmodule

module test_nor (
  input A, B,
  output Y
);
  assign Y = !(B | A);
endmodule
