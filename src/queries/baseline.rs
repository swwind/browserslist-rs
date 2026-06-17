use super::{Distrib, QueryResult};
use crate::{opts::Opts, semver::Version};
use browserslist_data::{baseline, caniuse};

fn baseline_query(
    get_min_version: impl Fn(&str) -> Option<&'static str>,
    opts: &Opts,
    downstream: bool,
    kaios: bool,
) -> QueryResult {
    // Collect core browser min versions
    let mut core_mins: Vec<(&'static str, &'static str)> = Vec::new();

    let mut distribs: Vec<Distrib> = caniuse::iter_browser_stat(opts.mobile_to_desktop)
        .flat_map(|(name, version_list)| {
            let min_semver =
                get_min_version(name).map(|v| v.parse::<Version>().unwrap_or_default());
            if let Some(min_v) = get_min_version(name) {
                core_mins.push((name, min_v));
            }
            version_list
                .iter()
                .filter(move |v| {
                    let Some(ref min) = min_semver else {
                        return false;
                    };
                    v.released && v.version().parse::<Version>().unwrap_or_default() >= *min
                })
                .map(move |v| Distrib::new(name, v.version()))
        })
        .collect();

    if downstream {
        let chrome_min = core_mins
            .iter()
            .find(|(b, _)| *b == "chrome")
            .map(|(_, v)| *v);
        let firefox_min = core_mins
            .iter()
            .find(|(b, _)| *b == "firefox")
            .map(|(_, v)| *v);

        // Add Blink downstream browsers
        if let Some(chrome_min) = chrome_min {
            for ds_browser in baseline::blink_downstream_browsers() {
                if let Some(min_v) =
                    baseline::get_downstream_blink_min_version(chrome_min, ds_browser)
                {
                    let min_semver = min_v.parse::<Version>().unwrap_or_default();
                    if let Some((_, version_list)) =
                        caniuse::get_browser_stat(ds_browser, opts.mobile_to_desktop)
                    {
                        for v in version_list.iter().filter(|v| {
                            v.released
                                && v.version().parse::<Version>().unwrap_or_default() >= min_semver
                        }) {
                            distribs.push(Distrib::new(ds_browser, v.version()));
                        }
                    }
                }
            }
        }

        // Add Gecko downstream browsers (KaiOS requires explicit opt-in)
        if let Some(firefox_min) = firefox_min {
            for ds_browser in baseline::gecko_downstream_browsers() {
                if ds_browser == "kaios" && !kaios {
                    continue;
                }
                if let Some(min_v) =
                    baseline::get_downstream_gecko_min_version(firefox_min, ds_browser)
                {
                    let min_semver = min_v.parse::<Version>().unwrap_or_default();
                    if let Some((_, version_list)) =
                        caniuse::get_browser_stat(ds_browser, opts.mobile_to_desktop)
                    {
                        for v in version_list.iter().filter(|v| {
                            v.released
                                && v.version().parse::<Version>().unwrap_or_default() >= min_semver
                        }) {
                            distribs.push(Distrib::new(ds_browser, v.version()));
                        }
                    }
                }
            }
        }
    }

    Ok(distribs)
}

pub(super) fn baseline_widely(opts: &Opts, downstream: bool, kaios: bool) -> QueryResult {
    baseline_query(
        baseline::get_baseline_widely_min_version,
        opts,
        downstream,
        kaios,
    )
}

pub(super) fn baseline_newly(opts: &Opts, downstream: bool, kaios: bool) -> QueryResult {
    baseline_query(
        baseline::get_baseline_newly_min_version,
        opts,
        downstream,
        kaios,
    )
}

pub(super) fn baseline_year(year: u16, opts: &Opts, downstream: bool, kaios: bool) -> QueryResult {
    baseline_query(
        |browser| baseline::get_baseline_year_min_version(year, browser),
        opts,
        downstream,
        kaios,
    )
}

/// Compute cutoff = widelyAvailableOnDate - 30 months, then query by that cutoff date.
pub(super) fn baseline_widely_on_date(
    date: &str,
    opts: &Opts,
    downstream: bool,
    kaios: bool,
) -> QueryResult {
    let cutoff = subtract_30_months(date)?;
    baseline_query(
        |browser| baseline::get_baseline_cutoff_date_min_version(&cutoff, browser),
        opts,
        downstream,
        kaios,
    )
}

fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

/// Subtract 30 months from a YYYY-MM-DD date string, matching JS `Date.setMonth` overflow semantics.
fn subtract_30_months(date: &str) -> Result<String, crate::error::Error> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(crate::error::Error::UnknownQuery(
            format!("invalid date: {date}").into(),
        ));
    }
    let year: i32 = parts[0].parse().unwrap_or(0);
    let month: i32 = parts[1].parse().unwrap_or(1);
    let day: i32 = parts[2].parse().unwrap_or(1);

    let total_months = year * 12 + (month - 1) - 30;
    let mut new_year = total_months / 12;
    let mut new_month = total_months % 12 + 1;

    let cap = days_in_month(new_year, new_month);
    let new_day = if day > cap {
        let overflow = day - cap;
        if new_month == 12 {
            new_year += 1;
            new_month = 1;
        } else {
            new_month += 1;
        }
        overflow
    } else {
        day
    };

    Ok(format!("{:04}-{:02}-{:02}", new_year, new_month, new_day))
}

#[cfg(test)]
mod tests {
    use crate::test::run_compare;
    use crate::opts::Opts;
    use test_case::test_case;

    #[test_case("baseline widely available"; "widely")]
    #[test_case("BASELINE WIDELY AVAILABLE"; "case insensitive widely")]
    #[test_case("baseline newly available"; "newly")]
    #[test_case("baseline 2015"; "year 2015")]
    #[test_case("baseline 2016"; "year 2016")]
    #[test_case("baseline 2017"; "year 2017")]
    #[test_case("baseline 2018"; "year 2018")]
    #[test_case("baseline 2019"; "year 2019")]
    #[test_case("baseline 2020"; "year 2020")]
    #[test_case("baseline 2021"; "year 2021")]
    #[test_case("baseline 2022"; "year 2022")]
    #[test_case("baseline 2023"; "year 2023")]
    #[test_case("baseline 2024"; "year 2024")]
    #[test_case("baseline widely available on 2021-01-01"; "widely on date 2021-01-01")]
    #[test_case("baseline widely available on 2023-04-05"; "widely on date 2023-04-05")]
    #[test_case("baseline widely available on 2024-06-15"; "widely on date 2024-06-15")]
    #[test_case("baseline 2022 with downstream"; "year with downstream")]
    #[test_case("baseline widely available with downstream"; "widely with downstream")]
    #[test_case("baseline 2020 with downstream including kaios"; "year with downstream and kaios")]
    fn valid(query: &str) {
        run_compare(query, &Opts::default(), None);
    }
}
