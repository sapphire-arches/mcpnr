/*
 *  mcpnr
 *
 *  Copyright (C) 2021  Reed Koser <github@cakesoft.pw>
 *
 *  Based on yosys synth.cc:
 *  Copyright (C) 2012  Claire Xenia Wolf <claire@yosyshq.com>
 *
 *  Permission to use, copy, modify, and/or distribute this software for any
 *  purpose with or without fee is hereby granted, provided that the above
 *  copyright notice and this permission notice appear in all copies.
 *
 *  THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
 *  WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 *  MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
 *  ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 *  WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 *  ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 *  OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 *
 */

#include "kernel/yosys.h"

#if !defined(YOSYS_ENABLE_ABC)
#  error "ABC support is required for techmapping"
#endif

#define MC_TECHLIB_DIR_DFL "techlib"

USING_YOSYS_NAMESPACE
PRIVATE_NAMESPACE_BEGIN

struct SynthMcPass : public ScriptPass {
  SynthMcPass() : ScriptPass("synth_mc", "Synthesis script to minecraft gates") { }

  void help() override
  {
    //   |---v---|---v---|---v---|---v---|---v---|---v---|---v---|---v---|---v---|---v---|
    log("\n");
    log("    synth_mc [options]\n");
    log("\n");
    log("This command runs synthesis to minecraft logic gates. This command does not\n");
    log("operate on partly selected designs.\n");
    log("\n");
    log("    -top <module>\n");
    log("        use the specified module as top module (default='top')\n");
    log("\n");
    log("    -auto-top\n");
    log("        automatically determine the top of the design hierarchy\n");
    log("\n");
    log("    -flatten\n");
    log("        flatten the design before synthesis. this will pass '-auto-top' to\n");
    log("        'hierarchy' if no top module is specified.\n");
    log("\n");
    log("    -encfile <file>\n");
    log("        passed to 'fsm_recode' via 'fsm'\n");
    log("\n");
    log("    -nofsm\n");
    log("        do not run FSM optimization\n");
    log("\n");
    log("    -nordff\n");
    log("        passed to 'memory'. prohibits merging of FFs into memory read ports\n");
    log("\n");
    log("    -noshare\n");
    log("        do not run SAT-based resource sharing\n");
    log("\n");
    log("    -techlib <path>\n");
    log("        Path to the MCPNR techlib.\n");
    log("        Defaults to: " MC_TECHLIB_DIR_DFL "\n");
    log("\n");
    log("    -run <from_label>[:<to_label>]\n");
    log("        only run the commands between the labels (see below). an empty\n");
    log("        from label is synonymous to 'begin', and empty to label is\n");
    log("        synonymous to the end of the command list.\n");
    log("\n");
    log("\n");
    log("The following commands are executed by this synthesis command:\n");
    help_script();
    log("\n");
  }

  string top_module, fsm_opts, memory_opts, techlib_path;
  bool autotop, flatten, nofsm, noshare;

  void clear_flags() override
  {
    top_module.clear();
    fsm_opts.clear();
    memory_opts.clear();

    autotop = false;
    flatten = false;
    nofsm = false;
    noshare = false;
    techlib_path = MC_TECHLIB_DIR_DFL;
  }

  void execute(std::vector<std::string> args, RTLIL::Design *design) override
  {
    string run_from, run_to;
    clear_flags();

    log_header(design, "Executing SYNTH_MC pass.\n");
    log_push();

    size_t argidx;
    for (argidx = 1; argidx < args.size(); argidx++)
    {
      if (args[argidx] == "-top" && argidx+1 < args.size()) {
        top_module = args[++argidx];
        continue;
      }
      if (args[argidx] == "-encfile" && argidx+1 < args.size()) {
        fsm_opts = " -encfile " + args[++argidx];
        continue;
      }
      if (args[argidx] == "-run" && argidx+1 < args.size()) {
        size_t pos = args[argidx+1].find(':');
        if (pos == std::string::npos) {
          run_from = args[++argidx];
          run_to = args[argidx];
        } else {
          run_from = args[++argidx].substr(0, pos);
          run_to = args[argidx].substr(pos+1);
        }
        continue;
      }
      if (args[argidx] == "-auto-top") {
        autotop = true;
        continue;
      }
      if (args[argidx] == "-flatten") {
        flatten = true;
        continue;
      }
      if (args[argidx] == "-techlib") {
        if (argidx + 1 >= args.size()) {
          log_cmd_error("-techlib must have an argument\n");
          continue;
        }
        techlib_path = args[++argidx];
        continue;
      }
      if (args[argidx] == "-nofsm") {
        nofsm = true;
        continue;
      }
      if (args[argidx] == "-nordff") {
        memory_opts += " -nordff";
        continue;
      }
      if (args[argidx] == "-noshare") {
        noshare = true;
        continue;
      }
      break;
    }
    extra_args(args, argidx, design);

    if (!design->full_selection())
      log_cmd_error("This command only operates on fully selected designs!\n");

    run_script(design, run_from, run_to);

    log_pop();
  }

  void script() override
  {
    if (check_label("begin"))
    {
      run("read_verilog -lib " + techlib_path + "/cells_sim.v");
      if (help_mode) {
        run("hierarchy -check [-top <top> | -auto-top]");
      } else {
        if (top_module.empty()) {
          if (flatten || autotop)
            run("hierarchy -check -auto-top");
          else
            run("hierarchy -check");
        } else
          run(stringf("hierarchy -check -top %s", top_module.c_str()));
      }
    }

    if (check_label("coarse"))
    {
      run("proc");
      if (help_mode || flatten)
        run("flatten", "  (if -flatten)");
      run("opt_expr");
      run("opt_clean");
      run("check");
      run("opt -nodffe -nosdff");
      if (!nofsm)
        run("fsm" + fsm_opts, "      (unless -nofsm)");
      run("opt");
      run("wreduce");
      run("peepopt");
      run("opt_clean");
      run("alumacc");
      if (!noshare)
        run("share", "    (unless -noshare)");
      run("opt");
      run("memory -nomap" + memory_opts);
      run("opt_clean");
    }

    if (check_label("fine"))
    {
      run("opt -fast -full");
      run("memory_map");
      run("opt -full");
      run("techmap");
      run("opt -fast");
      run("dfflibmap -liberty " + techlib_path + "/minecraft.lib");
      run("opt -fast");
      run("abc -liberty " + techlib_path + "/minecraft.lib");
      run("opt -fast");
    }

    if (check_label("check"))
    {
      run("stat");
      run("check");
    }
  }
} SynthMcPass;

PRIVATE_NAMESPACE_END
