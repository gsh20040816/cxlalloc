import sys
import copy

import dash
from dash import Dash, html, dcc, Input, Output, callback, ctx
import dash_bootstrap_components as dbc
import plotly.express as px
import polars as pl
from polars import selectors as cs


df = pl.read_ndjson(sys.argv[1])

TYPE_VARYING = "varying"
TYPE_STORE = "store"
ID_FIGURE = "figure"

SELECTORS = {}

OUTPUTS = ["process_id", "thread_id", "time"]
VARYING = []


CHOICES = [
    {"label": label, "value": value}
    for label, value in [
        ("Set as X-axis", "x"),
        ("Facet along row", "facet_row"),
        ("Facet along column", "facet_column"),
        ("Facet along color", "facet_color"),
    ]
]


def main():
    layout = []

    fixed = []
    varying = []

    for name, selector in recurse(df.drop(OUTPUTS)):
        values = unique(selector)

        if len(values) == 1:
            value = values[0]
            if type(value) is bool:
                value = "true" if value else "false"

            fixed.append(
                dbc.Row(
                    [
                        dbc.Col(html.Span(name)),
                        dbc.Col(dcc.Dropdown([value], value=value, disabled=True)),
                    ]
                )
            )
            continue
        else:
            SELECTORS[name] = selector
            VARYING.append(name)
            varying.append(
                dbc.Row(
                    [
                        dcc.Store(
                            id={"type": TYPE_STORE, "index": name},
                            storage_type="local",
                        ),
                        dbc.Col(html.Span(name)),
                        dbc.Col(
                            dcc.Dropdown(
                                CHOICES
                                + [
                                    {"label": f"Filter to {value}", "value": value}
                                    for value in values.to_list()
                                ],
                                id={"type": TYPE_VARYING, "index": name},
                            )
                        ),
                    ]
                )
            )

    layout.append(
        dbc.Row(
            [
                dbc.Col(fixed),
                dbc.Col(varying),
            ]
        )
    )

    layout.append(dcc.Graph(figure={}, id=ID_FIGURE))

    app = Dash(
        external_stylesheets=[dbc.themes.BOOTSTRAP],
    )
    app.layout = layout
    app.run(debug=True)


def recurse(df):
    def inner(columns, namespace, selector):
        select = pl.col if selector is None else lambda col: selector.struct.field(col)

        for col in columns:
            dtype = df.select(select(col)).to_series().dtype
            name = col if namespace == "" else f"{namespace}/{col}"

            if hasattr(dtype, "fields"):
                yield from inner(
                    [field.name for field in dtype.fields],
                    name,
                    select(col),
                )
            else:
                yield (name, select(col))

    yield from inner(df.columns, "", None)


def unique(field):
    return df.select(field).unique().to_series().sort()


@callback(
    Output({"type": TYPE_STORE, "index": dash.MATCH}, "data"),
    Output({"type": TYPE_VARYING, "index": dash.MATCH}, "value"),
    Input({"type": TYPE_STORE, "index": dash.MATCH}, "data"),
    Input({"type": TYPE_VARYING, "index": dash.MATCH}, "value"),
)
def update_store(data, varying):
    if varying is None:
        return data, data
    else:
        return varying, varying


@callback(
    Output(component_id=ID_FIGURE, component_property="figure"),
    Input({"type": TYPE_STORE, "index": dash.ALL}, "modified_timestamp"),
    dash.State({"type": TYPE_STORE, "index": dash.ALL}, "data"),
)
def update(
    ts,
    varying,
):
    if ts is None or any([value is None for value in varying]):
        raise dash.exceptions.PreventUpdate

    x = None
    facet_row = None
    facet_column = None
    facet_color = None
    filters = []

    # Validate
    for name, value in zip(VARYING, varying):
        selector = SELECTORS[name]
        if value == "x":
            if x is not None:
                return {}
            x = (name, selector)
        elif value == "facet_row":
            if facet_row is not None:
                return {}
            facet_row = (name, selector)
        elif value == "facet_column":
            if facet_column is not None:
                return {}
            facet_column = (name, selector)
        elif value == "facet_color":
            if facet_color is not None:
                return None
            facet_color = (name, selector)
        else:
            filters.append(selector == value)

    filtered = df.filter(*filters)

    filtered = (
        filtered.group_by(cs.exclude(*OUTPUTS))
        .agg(
            x[1].first().alias(x[0]),
            time_mean=pl.col("time").mean() / 1e6,
            time_std=pl.col("time").std() / 1e6,
        )
        .sort(x[1])
    )

    fig = px.line(
        filtered,
        x=x[0],
        y="time_mean",
        error_y="time_std",
        facet_row=facet_row[0] if facet_row is not None else None,
        facet_col=facet_column[0] if facet_column is not None else None,
        color=facet_color[0] if facet_color is not None else None,
        markers=True,
    )

    fig.update_xaxes(title_text=x[0], tickvals=filtered[x[0]].unique())
    fig.update_yaxes(title_text="Time (s)", autorangeoptions_include=0.0)
    return fig


if __name__ == "__main__":
    main()
