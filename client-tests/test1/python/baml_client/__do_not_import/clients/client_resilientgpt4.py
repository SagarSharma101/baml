# This file is generated by the BAML compiler.
# Do not edit this file directly.
# Instead, edit the BAML files and recompile.
#
# BAML version: 0.1.1-canary.12
# Generated Date: __DATE__
# Generated by: __USER__

# ruff: noqa: E501,F401
# flake8: noqa: E501,F401
# pylint: disable=unused-import,line-too-long
# fmt: off

from baml_core.provider_manager import LLMManager


ResilientGPT4 = LLMManager.add_llm(
    name="ResilientGPT4",
    provider="baml-fallback",
    retry_policy=None,
    options=dict(
        strategy=[{"client": "AZURE_DEFAULT"}, {"client": "AZURE_GPT4"}, {"client": "LARGE_RESPONSE"}],
    ),
)
