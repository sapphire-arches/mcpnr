#!/usr/bin/env python3

import argparse
import sys

def parse_args():
    parser = argparse.ArgumentParser(description='Yosys synthesis script generator for mcpnr test designs')
    parser.add_argument('--liberty', required=True, help='Path to Minecraft liberty file')
    parser.add_argument('--plugin', required=True, help='Path to the synth_mc plugin')
    parser.add_argument('--output', required=True, help='Name of file to which protobuf-formatted design will be written')
    parser.add_argument('--verilog', action='append', help='Verilog file to read (may be specified more than once)')
    parser.add_argument('out_file', help='Script file to write')

    return parser.parse_args()

def main():
    args = parse_args()

    if len(args.verilog) < 1:
        print('[!] Must specify at least 1 verilog file')
        sys.exit(1)
    with open(args.out_file, 'w') as o:
        for verilog in args.verilog:
            o.write('read_verilog ' + verilog + '\n')
        o.write('plugin -i ' + args.plugin + '\n')
        o.write('synth_mc -flatten -liberty ' + args.liberty + '\n')
        o.write('stat -liberty ' + args.liberty + '\n')
        o.write('write_protobuf ' + args.output + '\n')

if __name__ == '__main__':
    main()
