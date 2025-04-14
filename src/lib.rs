use wasm_bindgen::prelude::*;
use yew::prelude::*;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use statrs::distribution::{LogNormal, Normal, Continuous, ContinuousCDF};
use statrs::statistics::Distribution;

/// Structure to hold a pinned (fixed) distribution along with its computed call payoff.
#[derive(Clone, PartialEq, Debug)]
struct PinnedDistribution {
    mu: f64,
    sigma: f64,
    strike: f64,
    call_payoff: f64,
}

#[function_component(App)]
fn app() -> Html {
    // State hooks for the dynamic parameters.
    let mu = use_state(|| 0.05);
    let sigma = use_state(|| 0.25);
    let strike = use_state(|| 1.0);

    // State for pinned distributions.
    let pinned = use_state(|| Vec::<PinnedDistribution>::new());

    // Reference to the canvas element.
    let canvas_ref = use_node_ref();

    // Use an effect that re-draws the plot whenever any of the current parameters or the pinned list changes.
    {
        // Clone state for the effect.
        let mu_val = *mu;
        let sigma_val = *sigma;
        let strike_val = *strike;
        let pinned_cloned = pinned.clone();
        let canvas_ref = canvas_ref.clone();

        use_effect_with_deps(
            move |(mu_val, sigma_val, strike_val, pinned_list)| {
                if let Some(canvas) = canvas_ref.cast::<web_sys::HtmlCanvasElement>() {
                    if let Err(e) =
                        draw_plot(&canvas, *mu_val, *sigma_val, *strike_val, &pinned_list)
                    {
                        web_sys::console::error_1(&e.to_string().into());
                    }
                }
                move || {}
            },
            (mu_val, sigma_val, strike_val, pinned_cloned),
        );
    }

    // Input callbacks for dynamic parameters.
    let on_mu_input = {
        let mu = mu.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            if let Ok(val) = input.value().parse::<f64>() {
                mu.set(val);
            }
        })
    };
    let on_sigma_input = {
        let sigma = sigma.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            if let Ok(val) = input.value().parse::<f64>() {
                sigma.set(val);
            }
        })
    };
    let on_strike_input = {
        let strike = strike.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            if let Ok(val) = input.value().parse::<f64>() {
                strike.set(val);
            }
        })
    };

    // Callback for the "Pin Distribution" button: capture the current parameters and add a new pinned distribution.
    let on_pin = {
        let mu_val = *mu;
        let sigma_val = *sigma;
        let strike_val = *strike;
        let pinned = pinned.clone();
        Callback::from(move |_| {
            let d1 = (mu_val - strike_val.ln() + sigma_val * sigma_val) / sigma_val;
            let d2 = d1 - sigma_val;
            let norm = Normal::new(0.0, 1.0).unwrap();
            let call_payoff =
                (mu_val + sigma_val * sigma_val / 2.0).exp() * norm.cdf(d1) - strike_val * norm.cdf(d2);
            let mut new_pinned = (*pinned).clone();
            new_pinned.push(PinnedDistribution {
                mu: mu_val,
                sigma: sigma_val,
                strike: strike_val,
                call_payoff,
            });
            pinned.set(new_pinned);
        })
    };

    // Callback for removing a pinned distribution.
    let on_remove_pin = {
        let pinned = pinned.clone();
        Callback::from(move |index: usize| {
            let mut new_pinned = (*pinned).clone();
            if index < new_pinned.len() {
                new_pinned.remove(index);
                pinned.set(new_pinned);
            }
        })
    };

    html! {
        <div style="font-family: sans-serif; max-width: 800px; margin: 0 auto;">
            <h1>{ "Interactive Lognormal PDF & Call Payoff" }</h1>
            <div style="margin-bottom: 1em;">
                <label>{ "Mu: " }</label>
                <input type="number" step="0.01" value={(*mu).to_string()} oninput={on_mu_input} />
            </div>
            <div style="margin-bottom: 1em;">
                <label>{ "Sigma: " }</label>
                <input type="number" step="0.01" value={(*sigma).to_string()} oninput={on_sigma_input} />
            </div>
            <div style="margin-bottom: 1em;">
                <label>{ "Strike: " }</label>
                <input type="number" step="0.01" value={(*strike).to_string()} oninput={on_strike_input} />
            </div>
            <div style="margin-bottom: 1em;">
                <button onclick={on_pin}>{ "Pin Distribution" }</button>
            </div>
            <canvas ref={canvas_ref} width="640" height="480" style="border:1px solid #ccc;"></canvas>
            <div>
                <h2>{ "Pinned Distributions" }</h2>
                {
                    for (*pinned).iter().enumerate().map(|(i, pd)| {
                        let pd_clone = pd.clone();
                        let on_remove = {
                            let on_remove_pin = on_remove_pin.clone();
                            Callback::from(move |_| {
                                on_remove_pin.emit(i);
                            })
                        };
                        html! {
                            <div style="border: 1px solid #ccc; margin: 0.5em; padding: 0.5em;">
                                <span>
                                    { format!("Pin {}: mu = {:.2}, sigma = {:.2}, strike = {:.2}, call payoff = {:.4}",
                                              i+1, pd_clone.mu, pd_clone.sigma, pd_clone.strike, pd_clone.call_payoff) }
                                </span>
                                <button onclick={on_remove} style="margin-left: 1em;">{ "X" }</button>
                            </div>
                        }
                    })
                }
            </div>
        </div>
    }
}

/// Draws the graph on the supplied canvas. It plots:
/// - The current dynamic distribution (in blue) with the integration region (x ≥ strike) shaded red,
///   along with the computed call payoff annotation.
/// - Each pinned distribution's curve and area (using a distinct color), a vertical line at its strike,
///   and an annotation of its call payoff.
fn draw_plot(
    canvas: &web_sys::HtmlCanvasElement,
    mu: f64,
    sigma: f64,
    strike: f64,
    pinned: &Vec<PinnedDistribution>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a canvas backend.
    let backend = CanvasBackend::with_canvas_object(canvas.clone()).unwrap();
    let root = backend.into_drawing_area();
    root.fill(&WHITE)?;

    // Build the dynamic (current) distribution.
    let ln = LogNormal::new(mu, sigma)?;
    let xmin = 0.001;
    let xmax = ln.mean().unwrap_or(1.0) * 3.0;
    let n_points = 1000;
    let dx = (xmax - xmin) / n_points as f64;
    let mut density_data = Vec::with_capacity(n_points + 1);
    for i in 0..=n_points {
        let x = xmin + i as f64 * dx;
        density_data.push((x, ln.pdf(x)));
    }
    let max_pdf = density_data.iter().map(|&(_, y)| y).fold(0.0, f64::max);

    // Create the chart.
    let mut chart = ChartBuilder::on(&root)
        .caption("Interactive Lognormal PDF & Call Payoff Region", ("sans-serif", 20))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(40)
        .build_cartesian_2d(xmin..xmax, 0.0..max_pdf)?;
    chart.configure_mesh().x_desc("x").y_desc("PDF").draw()?;

    // Draw the dynamic distribution's density curve.
    chart.draw_series(LineSeries::new(density_data.clone(), &BLUE))?;

    // Shade the integration region for the dynamic distribution.
    let call_region: Vec<(f64, f64)> = density_data
        .iter()
        .cloned()
        .filter(|(x, _)| *x >= strike)
        .collect();
    let mut area_points = vec![(strike, 0.0)];
    area_points.extend(call_region);
    area_points.push((xmax, 0.0));
    chart.draw_series(std::iter::once(Polygon::new(
        area_points,
        RED.mix(0.2).filled(),
    )))?;
    // Draw a vertical line at the dynamic strike.
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(strike, 0.0), (strike, max_pdf)],
        &BLACK,
    )))?;

    // Annotate the dynamic call payoff.
    let d1 = (mu - strike.ln() + sigma * sigma) / sigma;
    let d2 = d1 - sigma;
    let norm = Normal::new(0.0, 1.0)?;
    let call_payoff = (mu + sigma * sigma / 2.0).exp() * norm.cdf(d1) - strike * norm.cdf(d2);
    chart.draw_series(std::iter::once(Text::new(
        format!("Call: {:.4}", call_payoff),
        (strike * 1.05, max_pdf * 0.8),
        ("sans-serif", 15),
    )))?;

    // Predefined fill styles for pinned distributions (cycled by index).
    let pinned_fill_styles = vec![
        RED.mix(0.3).filled(),
        GREEN.mix(0.3).filled(),
        BLUE.mix(0.3).filled(),
        CYAN.mix(0.3).filled(),
        MAGENTA.mix(0.3).filled(),
        RGBColor(255, 165, 0).mix(0.3).filled(), // Orange
        RGBColor(128, 0, 128).mix(0.3).filled(),  // Purple
    ];

    // Draw each pinned distribution.
    for (i, pd) in pinned.iter().enumerate() {
        let color = &pinned_fill_styles[i % pinned_fill_styles.len()];
        let ln_pin = LogNormal::new(pd.mu, pd.sigma)?;
        let pinned_xmin = 0.001;
        let pinned_xmax = ln_pin.mean().unwrap_or(1.0) * 3.0;
        let n_points = 1000;
        let dx = (pinned_xmax - pinned_xmin) / n_points as f64;
        let mut pinned_data = Vec::with_capacity(n_points + 1);
        for j in 0..=n_points {
            let x = pinned_xmin + j as f64 * dx;
            pinned_data.push((x, ln_pin.pdf(x)));
        }
        // Draw the pinned density line.
        chart.draw_series(LineSeries::new(pinned_data.clone(), *color))?;
        // Shade the integration region for the pinned distribution.
        let pinned_region: Vec<(f64, f64)> = pinned_data
            .into_iter()
            .filter(|(x, _)| *x >= pd.strike)
            .collect();
        let mut area_points_pin = vec![(pd.strike, 0.0)];
        area_points_pin.extend(pinned_region);
        area_points_pin.push((pinned_xmax, 0.0));
        chart.draw_series(std::iter::once(Polygon::new(
            area_points_pin,
            ShapeStyle::from(&RGBColor(255, 0, 0)).filled(),
        )))?;
        // Draw a vertical line at the pinned strike.
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(pd.strike, 0.0), (pd.strike, max_pdf)],
            *color,
        )))?;
        // Annotate the pinned call payoff.
        chart.draw_series(std::iter::once(Text::new(
            format!("Pinned: {:.4}", pd.call_payoff),
            (pd.strike * 1.05, max_pdf * 0.7 - (i as f64 * max_pdf * 0.05)),
            ("sans-serif", 12),
        )))?;
    }

    root.present()?;
    Ok(())
}

#[wasm_bindgen(start)]
pub fn run_app() {
    yew::Renderer::<App>::new().render();
}
