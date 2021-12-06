module test_nor (
  input  [7:0] A, B,
  output [7:0] Y,
);
  assign Y = ~(B | A);
endmodule
