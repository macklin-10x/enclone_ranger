// Copyright (c) 2022 10X Genomics, Inc. All rights reserved.

use crate::proc_args2::proc_args_tail;
use crate::proc_args3::{get_path_fail, proc_meta, proc_meta_core, proc_xcr};
use crate::proc_args_check::check_cvars;
use enclone_core::defs::EncloneControl;
use enclone_core::tilde_expand_me;
use enclone_vars::encode_arith;
use evalexpr::build_operator_tree;
use expr_tools::vars_of_node;
use io_utils::{open_for_read, open_userfile_for_read, path_exists};
use std::collections::HashMap;
use std::io::BufRead;
use std::time::Instant;
use string_utils::{parse_csv, TextUtils};
use vector_utils::{bin_member, next_diff, sort_sync2, unique_sort};

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

// Parse joint barcode-level information file from BC_JOINT.

fn parse_bc_joint(ctl: &mut EncloneControl) -> Result<(), String> {
    let bc = &ctl.gen_opt.bc_joint;
    let delimiter = if bc.ends_with(".tsv") { '\t' } else { ',' };
    let n = ctl.origin_info.n();
    let mut origin_for_bc = vec![HashMap::<String, String>::new(); n];
    let mut donor_for_bc = vec![HashMap::<String, String>::new(); n];
    let mut tag = vec![HashMap::<String, String>::new(); n];
    let mut barcode_color = vec![HashMap::<String, String>::new(); n];
    let mut alt_bc_fields = vec![Vec::<(String, HashMap<String, String>)>::new(); n];
    let f = open_userfile_for_read(bc);
    let mut first = true;
    let mut fieldnames = Vec::<String>::new();
    let mut dataset_pos = 0;
    let mut barcode_pos = 0;
    let (mut origin_pos, mut donor_pos, mut tag_pos, mut color_pos) = (None, None, None, None);
    let mut to_alt = Vec::<isize>::new();
    let mut to_origin_pos = HashMap::<String, usize>::new();
    for i in 0..ctl.origin_info.n() {
        to_origin_pos.insert(ctl.origin_info.dataset_id[i].clone(), i);
    }
    for line in f.lines() {
        let s = line.unwrap();
        if first {
            let fields = s.split(delimiter).collect::<Vec<&str>>();
            to_alt = vec![-1_isize; fields.len()];
            if !fields.contains(&"dataset") {
                return Err(format!("\nThe file\n{bc}\nis missing the dataset field.\n",));
            }
            if !fields.contains(&"barcode") {
                return Err(format!("\nThe file\n{bc}\nis missing the barcode field.\n",));
            }
            for x in fields.iter() {
                fieldnames.push(x.to_string());
            }
            for (i, field) in fields.into_iter().enumerate() {
                if field == "color" {
                    color_pos = Some(i);
                }
                if field == "barcode" {
                    barcode_pos = i;
                } else if field == "dataset" {
                    dataset_pos = i;
                } else if field == "origin" {
                    origin_pos = Some(i);
                } else if field == "donor" {
                    donor_pos = Some(i);
                } else if field == "tag" {
                    tag_pos = Some(i);
                } else {
                    to_alt[i] = alt_bc_fields[0].len() as isize;
                    for li in alt_bc_fields.iter_mut().take(ctl.origin_info.n()) {
                        li.push((field.to_string(), HashMap::<String, String>::new()));
                    }
                }
            }
            first = false;
        } else {
            let fields = s.split(delimiter).collect::<Vec<&str>>();
            if fields.len() != fieldnames.len() {
                return Err(format!(
                    "\nThere is a line\n{}\nin {}\n\
                     that has {} fields, which isn't right, because the header line \
                     has {} fields.\n",
                    s,
                    bc,
                    fields.len(),
                    fieldnames.len(),
                ));
            }
            let dataset = fields[dataset_pos].to_string();
            if !to_origin_pos.contains_key(&dataset) {
                return Err(format!(
                    "\nIn the file\n{bc},\nthe value\n{dataset}\nis found for dataset, however that is \
                     not an abbreviated dataset name.\n",
                ));
            }
            let li = to_origin_pos[&dataset];
            for i in 0..fields.len() {
                if to_alt[i] >= 0 {
                    alt_bc_fields[li][to_alt[i] as usize]
                        .1
                        .insert(fields[barcode_pos].to_string(), fields[i].to_string());
                }
            }
            if !fields[barcode_pos].contains('-') {
                return Err(format!(
                    "\nThe barcode \"{}\" appears in the file\n{bc}.\n\
                     That doesn't make sense because a barcode\nshould include a hyphen.\n",
                    fields[barcode_pos],
                ));
            }

            if let Some(origin_pos) = origin_pos {
                origin_for_bc[li].insert(
                    fields[barcode_pos].to_string(),
                    fields[origin_pos].to_string(),
                );
            }
            if let Some(donor_pos) = donor_pos {
                donor_for_bc[li].insert(
                    fields[barcode_pos].to_string(),
                    fields[donor_pos].to_string(),
                );
            }
            if let Some(tag_pos) = tag_pos {
                tag[li].insert(fields[barcode_pos].to_string(), fields[tag_pos].to_string());
            }
            if let Some(color_pos) = color_pos {
                barcode_color[li].insert(
                    fields[barcode_pos].to_string(),
                    fields[color_pos].to_string(),
                );
            }
        }
    }
    ctl.origin_info.origin_for_bc = origin_for_bc;
    ctl.origin_info.donor_for_bc = donor_for_bc;
    ctl.origin_info.tag = tag;
    ctl.origin_info.barcode_color = barcode_color;
    ctl.origin_info.alt_bc_fields = alt_bc_fields;
    Ok(())
}

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

pub fn proc_args_post(
    ctl: &mut EncloneControl,
    args: &[String],
    metas: &[String],
    metaxs: &[String],
    xcrs: &[String],
    have_gex: bool,
    gex: &str,
    bc: &str,
    using_plot: bool,
) -> Result<(), String> {
    // Process INFO.

    let t = Instant::now();
    if ctl.gen_opt.info.is_some() {
        let f = open_for_read![&ctl.gen_opt.info.as_ref().unwrap()];
        let mut lines = Vec::<String>::new();
        for line in f.lines() {
            let s = line.unwrap();
            lines.push(s);
        }
        if lines.is_empty() {
            return Err(format!(
                "\nThe file {} is empty.\n",
                ctl.gen_opt.info.as_ref().unwrap()
            ));
        }
        let fields = lines[0].split(',').collect::<Vec<&str>>();
        if !fields.contains(&"vj_seq1") || !fields.contains(&"vj_seq2") {
            return Err(format!(
                "\nThe CSV file {} needs to have fields vj_seq1 and vj_seq2.\n",
                ctl.gen_opt.info.as_ref().unwrap()
            ));
        }
        for &field in &fields {
            if field != "vj_seq1" && field != "vj_seq2" {
                ctl.gen_opt.info_fields.push(field.to_string());
                ctl.gen_opt.info_fields.push(format!("log10({field})"));
            }
        }
        let mut tags = Vec::<String>::new();
        for (i, line) in lines.iter().enumerate().skip(1) {
            let vals = parse_csv(line);
            if vals.len() != fields.len() {
                eprintln!(
                    "\nINFO file line {} has length {} whereas the file has {} fields. \
                    The line is\n{}\n",
                    i + 1,
                    vals.len(),
                    fields.len(),
                    line
                );
            }
            let (mut vj1, mut vj2) = (String::new(), String::new());
            let mut other = Vec::<String>::new();
            for i in 0..vals.len() {
                if fields[i] == "vj_seq1" {
                    vj1 = vals[i].to_string();
                } else if fields[i] == "vj_seq2" {
                    vj2 = vals[i].to_string();
                } else {
                    other.push(vals[i].to_string());
                    let mut log10_val = "".to_string();
                    if vals[i].parse::<f64>().is_ok() {
                        let val = vals[i].force_f64();
                        if val > 0.0 {
                            log10_val = format!("{:.2}", val.log10());
                        }
                    }
                    other.push(log10_val);
                }
            }
            let tag = format!("{vj1}_{vj2}");
            if ctl.gen_opt.info_resolve && ctl.gen_opt.info_data.contains_key(&tag) {
                continue;
            }
            tags.push(tag.clone());
            sort_sync2(&mut ctl.gen_opt.info_fields, &mut other);
            ctl.gen_opt.info_data.insert(tag, other);
        }
        tags.sort();
        let mut i = 0;
        while i < tags.len() {
            let j = next_diff(&tags, i);
            if j - i > 1 {
                return Err(format!(
                    "\nThe immune receptor sequence pair\n{},\n {}\nappears more than once \
                    in the file {}.\n",
                    tags[i].before("_"),
                    tags[i].after("_"),
                    ctl.gen_opt.info.as_ref().unwrap(),
                ));
            }
            i = j;
        }
    }

    // Expand ~ and ~user in output file names.

    let mut files = [
        &mut ctl.plot_opt.plot_file,
        &mut ctl.gen_opt.fasta_filename,
        &mut ctl.gen_opt.fasta_aa_filename,
        &mut ctl.gen_opt.dref_file,
        &mut ctl.parseable_opt.pout,
    ];
    for f in files.iter_mut() {
        tilde_expand_me(f);
    }

    // Test VAR_DEF arguments for circularity.

    let mut var_def_vars = Vec::<Vec<String>>::new();
    let n = ctl.gen_opt.var_def.len();
    for i in 0..n {
        let x = &ctl.gen_opt.var_def[i].2;
        var_def_vars.push(vars_of_node(x));
    }
    let mut edges = Vec::<(usize, usize)>::new();
    for (i, vari) in ctl.gen_opt.var_def.iter().take(n).enumerate() {
        for (j, varj) in var_def_vars.iter().take(n).enumerate() {
            if bin_member(varj, &vari.0) {
                edges.push((i, j));
            }
        }
    }
    let mut reach = vec![vec![false; n]; n];
    loop {
        let mut progress = false;
        for &(i, j) in &edges {
            if !reach[i][j] {
                reach[i][j] = true;
                progress = true;
            }
            for l in 0..n {
                if reach[l][i] && !reach[l][j] {
                    reach[l][j] = true;
                    progress = true;
                }
                if reach[j][l] && !reach[i][l] {
                    reach[i][l] = true;
                    progress = true;
                }
            }
        }
        if !progress {
            break;
        }
    }
    for (i, r) in reach.into_iter().enumerate().take(n) {
        if r[i] {
            return Err(
                "\nVAR_DEF arguments define a circular chain of dependencies.\n".to_string(),
            );
        }
    }

    // Substitute VAR_DEF into VAR_DEF.

    loop {
        let mut progress = false;
        for i in 0..n {
            for (j, var_def_j) in var_def_vars.iter_mut().enumerate().take(n) {
                if bin_member(var_def_j, &ctl.gen_opt.var_def[i].0) {
                    let sub = encode_arith(&ctl.gen_opt.var_def[i].0);
                    ctl.gen_opt.var_def[j].1 = ctl.gen_opt.var_def[j]
                        .1
                        .replace(&sub, &format!("({})", ctl.gen_opt.var_def[i].1));
                    ctl.gen_opt.var_def[j].2 =
                        build_operator_tree(&ctl.gen_opt.var_def[j].1).unwrap();
                    let x = &ctl.gen_opt.var_def[j].2;
                    *var_def_j = vars_of_node(x);
                    progress = true;
                }
            }
        }
        if !progress {
            break;
        }
    }

    // Substitute VAR_DEF into ALL_BC.

    for i in 0..ctl.gen_opt.all_bc_fields.len() {
        for j in 0..ctl.gen_opt.var_def.len() {
            if ctl.gen_opt.all_bc_fields[i] == ctl.gen_opt.var_def[j].0 {
                ctl.gen_opt.all_bc_fields[i] = ctl.gen_opt.var_def[j].3.clone();
                break;
            }
        }
    }

    // Sanity check grouping arguments.

    if ctl.clono_group_opt.style == "asymmetric"
        && (ctl.clono_group_opt.asymmetric_center.is_empty()
            || ctl.clono_group_opt.asymmetric_dist_formula.is_empty()
            || ctl.clono_group_opt.asymmetric_dist_bound.is_empty())
    {
        return Err(
            "\nIf the AGROUP option is used to specify asymmetric grouping, then all\n\
            of the options AG_CENTER, AG_DIST_FORMULA and AG_DIST_BOUND must also be \
            specified.\n"
                .to_string(),
        );
    }
    if (!ctl.clono_group_opt.asymmetric_center.is_empty()
        || !ctl.clono_group_opt.asymmetric_dist_formula.is_empty()
        || !ctl.clono_group_opt.asymmetric_dist_bound.is_empty())
        && ctl.clono_group_opt.style == "symmetric"
    {
        return Err("\nIf any of the asymmetric grouping options AG_CENTER or \
                AG_DIST_FORMULA or\nAG_DIST_BOUND are specified, then the option AGROUP \
                must also be specified, to turn on asymmetric grouping.\n"
            .to_string());
    }
    if ctl.clono_group_opt.style == "asymmetric" {
        if ctl.clono_group_opt.asymmetric_center != "from_filters"
            && ctl.clono_group_opt.asymmetric_center != "copy_filters"
        {
            return Err(
                "\nThe only allowed forms for AG_CENTER are AG_CENTER=from_filters\n\
                and AG_CENTER=copy_filters.\n"
                    .to_string(),
            );
        }
        if ctl.clono_group_opt.asymmetric_dist_formula != "cdr3_edit_distance" {
            return Err(
                "\nThe only allowed form for AG_DIST_FORMULA is cdr3_edit_distance.\n".to_string(),
            );
        }
        let ok1 = ctl
            .clono_group_opt
            .asymmetric_dist_bound
            .starts_with("top=")
            && ctl
                .clono_group_opt
                .asymmetric_dist_bound
                .after("top=")
                .parse::<usize>()
                .is_ok();
        let ok2 = ctl
            .clono_group_opt
            .asymmetric_dist_bound
            .starts_with("max=")
            && ctl
                .clono_group_opt
                .asymmetric_dist_bound
                .after("max=")
                .parse::<f64>()
                .is_ok();
        if !ok1 && !ok2 {
            return Err(
                "\nThe only allowed forms for AG_DIST_BOUND are top=n, where n is an\n\
                integer, and max=d, where d is a number.\n"
                    .to_string(),
            );
        }
    }

    // Sanity check other arguments (and more below).

    if !ctl.parseable_opt.pcols_show.is_empty()
        && ctl.parseable_opt.pcols_show.len() != ctl.parseable_opt.pcols.len()
    {
        return Err(
            "\nThe number of fields provided to PCOLS_SHOW has to match that for PCOLS.\n"
                .to_string(),
        );
    }
    if ctl.plot_opt.split_plot_by_dataset && ctl.plot_opt.split_plot_by_origin {
        return Err(
            "\nOnly one of SPLIT_PLOT_BY_DATASET and SPLIT_PLOT_BY_ORIGIN can be specified.\n"
                .to_string(),
        );
    }
    if ctl.clono_print_opt.amino.is_empty() && ctl.clono_print_opt.cvars.is_empty() {
        return Err(
            "\nSorry, use of both CVARS= and AMINO= (setting both to null) is not \
            supported.\n"
                .to_string(),
        );
    }
    if ctl.parseable_opt.pchains.parse::<usize>().is_err() && ctl.parseable_opt.pchains != "max" {
        return Err(
            "\nThe only allowed values for PCHAINS are a positive integer and max.\n".to_string(),
        );
    }
    if ctl.gen_opt.align_jun_align_consistency && ctl.pretty {
        return Err(
            "\nIf you use ALIGN_JALIGN_CONSISTENCY, you should also use PLAIN.\n".to_string(),
        );
    }
    if ctl.gen_opt.gene_scan_exact && ctl.gen_opt.gene_scan_test.is_none() {
        return Err(
            "\nIt doesn't make sense to specify SCAN_EXIT unless SCAN is also specified.\n"
                .to_string(),
        );
    }
    if ctl.clono_print_opt.conx && ctl.clono_print_opt.conp {
        return Err("\nPlease specify at most one of CONX and CONP.\n".to_string());
    }
    if ctl.clono_filt_opt.cdr3.is_some() && !ctl.clono_filt_opt.cdr3_lev.is_empty() {
        return Err(
            "\nPlease use the CDR3 argument to specify either a regular expression or a\n\
            Levenshtein distance pattern, but not both.\n"
                .to_string(),
        );
    }
    if ctl.gen_opt.clustal_aa != *""
        && ctl.gen_opt.clustal_aa != *"stdout"
        && !ctl.gen_opt.clustal_aa.ends_with(".tar")
    {
        return Err(
            "\nIf the value of CLUSTAL_AA is not stdout, it must end in .tar.\n".to_string(),
        );
    }
    if ctl.gen_opt.clustal_dna != *""
        && ctl.gen_opt.clustal_dna != *"stdout"
        && !ctl.gen_opt.clustal_dna.ends_with(".tar")
    {
        return Err(
            "\nIf the value of CLUSTAL_DNA is not stdout, it must end in .tar.\n".to_string(),
        );
    }
    if ctl.gen_opt.phylip_aa != *""
        && ctl.gen_opt.phylip_aa != *"stdout"
        && !ctl.gen_opt.phylip_aa.ends_with(".tar")
    {
        return Err(
            "\nIf the value of PHYLIP_AA is not stdout, it must end in .tar.\n".to_string(),
        );
    }
    if ctl.gen_opt.phylip_dna != *""
        && ctl.gen_opt.phylip_dna != *"stdout"
        && !ctl.gen_opt.phylip_dna.ends_with(".tar")
    {
        return Err(
            "\nIf the value of PHYLIP_DNA is not stdout, it must end in .tar.\n".to_string(),
        );
    }
    if ctl.clono_filt_opt_def.umi_filt && ctl.clono_filt_opt_def.umi_filt_mark {
        return Err(
            "\nIf you use UMI_FILT_MARK, you should also use NUMI, to turn off \
            the filter,\nas otherwise nothing will be marked.\n"
                .to_string(),
        );
    }
    if ctl.clono_filt_opt_def.umi_ratio_filt && ctl.clono_filt_opt_def.umi_ratio_filt_mark {
        return Err(
            "\nIf you use UMI_RATIO_FILT_MARK, you should also use NUMI_RATIO, to turn off \
            the filter,\nas otherwise nothing will be marked.\n"
                .to_string(),
        );
    }
    ctl.perf_stats(&t, "after main args loop 1");

    // Process TCR, BCR and META.

    let t = Instant::now();
    check_cvars(ctl)?;
    if !metas.is_empty() {
        let mut v = Vec::<String>::with_capacity(metas.len());
        for meta in metas {
            let f = get_path_fail(meta, ctl, "META")?;
            if f.contains('/') {
                let d = f.rev_before("/").to_string();
                if !ctl.gen_opt.pre.contains(&d) {
                    ctl.gen_opt.pre.push(d);
                }
            }
            v.push(f);
        }
        proc_meta(&v, ctl)?;
    }
    if !metaxs.is_empty() {
        let lines: Vec<_> = metaxs[metaxs.len() - 1]
            .split(';')
            .map(str::to_string)
            .collect();
        proc_meta_core(&lines, ctl)?;
    }
    ctl.perf_stats(&t, "in proc_meta");
    if !xcrs.is_empty() {
        let arg = &xcrs[xcrs.len() - 1];
        proc_xcr(arg, gex, bc, have_gex, ctl)?;
    }

    // Process BC_JOINT.

    if !ctl.gen_opt.bc_joint.is_empty() {
        parse_bc_joint(ctl)?;
    }

    // More argument sanity checking.

    let t = Instant::now();
    if ctl.clono_filt_opt.dataset.is_some() {
        let d = &ctl.clono_filt_opt.dataset.as_ref().unwrap();
        for x in d.iter() {
            if !ctl.origin_info.dataset_id.contains(x) {
                return Err(format!(
                    "\nDATASET argument has {} in it, which is not a known \
                    dataset name.\n",
                    *x
                ));
            }
        }
    }
    let bcr_only = [
        "PEER_GROUP",
        "PG_READABLE",
        "PG_DIST",
        "COLOR=peer",
        "CONST_IGH",
        "CONST_IGL",
    ];
    if !ctl.gen_opt.bcr {
        for arg in &args[1..] {
            for x in bcr_only.iter() {
                if arg == x || arg.starts_with(&format!("{x}=")) {
                    return Err(format!("\nThe option {x} does not make sense for TCR.\n"));
                }
            }
        }
    }

    // Proceed.

    for i in 0..ctl.origin_info.n() {
        let (mut cells_cr, mut rpc_cr) = (None, None);
        if ctl.gen_opt.internal_run {
            let p = &ctl.origin_info.dataset_path[i];
            let mut f = format!("{p}/metrics_summary_csv.csv");
            if !path_exists(&f) {
                f = format!("{p}/metrics_summary.csv");
            }
            if path_exists(&f) {
                let f = open_userfile_for_read(&f);
                let mut count = 0;
                let (mut cells_field, mut rpc_field) = (None, None);
                for line in f.lines() {
                    count += 1;
                    let s = line.unwrap();
                    let fields = parse_csv(&s);
                    for (i, x) in fields.iter().enumerate() {
                        if count == 1 {
                            if *x == "Estimated Number of Cells" {
                                cells_field = Some(i);
                            } else if *x == "Mean Read Pairs per Cell" {
                                rpc_field = Some(i);
                            }
                        } else if count == 2 {
                            if Some(i) == cells_field {
                                let mut n = x.to_string();
                                if n.contains('\"') {
                                    n = n.between("\"", "\"").to_string();
                                }
                                n = n.replace(',', "");
                                cells_cr = Some(n.force_usize());
                            } else if Some(i) == rpc_field {
                                let mut n = x.to_string();
                                if n.contains('\"') {
                                    n = n.between("\"", "\"").to_string();
                                }
                                n = n.replace(',', "");
                                rpc_cr = Some(n.force_usize());
                            }
                        }
                    }
                }
            }
        }
        ctl.origin_info.cells_cellranger.push(cells_cr);
        ctl.origin_info
            .mean_read_pairs_per_cell_cellranger
            .push(rpc_cr);
    }
    if ctl.plot_opt.plot_by_isotype {
        if using_plot || ctl.plot_opt.use_legend {
            return Err("\nPLOT_BY_ISOTYPE cannot be used with PLOT or LEGEND.\n".to_string());
        }
        if !ctl.gen_opt.bcr {
            return Err("\nPLOT_BY_ISOTYPE can only be used with BCR data.\n".to_string());
        }
        if ctl.plot_opt.plot_by_mark {
            return Err(
                "\nPLOT_BY_ISOTYPE and PLOT_BY_MARK cannot be used together.\n".to_string(),
            );
        }
    }
    if ctl.plot_opt.plot_by_mark && (using_plot || ctl.plot_opt.use_legend) {
        return Err("\nPLOT_BY_MARK cannot be used with PLOT or LEGEND.\n".to_string());
    }
    if ctl.parseable_opt.pbarcode && ctl.parseable_opt.pout.is_empty() {
        return Err(
            "\nIt does not make sense to specify PCELL unless POUT is also specified.\n"
                .to_string(),
        );
    }
    let mut donors = Vec::<String>::new();
    let mut origins = Vec::<String>::new();
    let mut tags = Vec::<String>::new();
    let mut origin_for_bc = Vec::<String>::new();
    let mut donor_for_bc = Vec::<String>::new();
    for i in 0..ctl.origin_info.n() {
        for x in ctl.origin_info.origin_for_bc[i].iter() {
            origins.push(x.1.clone());
            origin_for_bc.push(x.1.clone());
        }
        for x in ctl.origin_info.donor_for_bc[i].iter() {
            donors.push(x.1.clone());
            donor_for_bc.push(x.1.clone());
        }
        for x in ctl.origin_info.tag[i].iter() {
            tags.push((x.1).clone());
        }
        donors.push(ctl.origin_info.donor_id[i].clone());
        origins.push(ctl.origin_info.origin_id[i].clone());
    }
    unique_sort(&mut donors);
    unique_sort(&mut origins);
    unique_sort(&mut tags);
    unique_sort(&mut origin_for_bc);
    unique_sort(&mut donor_for_bc);
    ctl.origin_info.donors = donors.len();
    ctl.origin_info.dataset_list = ctl.origin_info.dataset_id.clone();
    unique_sort(&mut ctl.origin_info.dataset_list);
    ctl.origin_info.origin_list = origins.clone();
    ctl.origin_info.donor_list = donors.clone();
    ctl.origin_info.tag_list = tags;
    for i in 0..ctl.origin_info.donor_for_bc.len() {
        if !ctl.origin_info.donor_for_bc[i].is_empty() {
            ctl.clono_filt_opt_def.donor = true;
        }
    }
    ctl.perf_stats(&t, "after main args loop 2");
    proc_args_tail(ctl, args)?;

    // Sort chains_to_align.

    unique_sort(&mut ctl.gen_opt.chains_to_align);
    unique_sort(&mut ctl.gen_opt.chains_to_align2);
    unique_sort(&mut ctl.gen_opt.chains_to_jun_align);
    unique_sort(&mut ctl.gen_opt.chains_to_jun_align2);

    // Check for invalid variables in linear conditions.

    for i in 0..ctl.clono_filt_opt.bounds.len() {
        ctl.clono_filt_opt.bounds[i].require_valid_variables(ctl)?;
    }
    if ctl.gen_opt.gene_scan_test.is_some() {
        ctl.gen_opt
            .gene_scan_test
            .as_ref()
            .unwrap()
            .require_valid_variables(ctl)?;
        ctl.gen_opt
            .gene_scan_control
            .as_ref()
            .unwrap()
            .require_valid_variables(ctl)?;
    }
    Ok(())
}
