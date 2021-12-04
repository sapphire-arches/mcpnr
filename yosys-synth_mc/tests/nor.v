module test_nor (
  input A, B,
  output Y
);
  assign Y = !(B | A);
endmodule
