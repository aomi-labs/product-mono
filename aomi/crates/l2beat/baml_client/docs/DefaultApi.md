# \DefaultApi

All URIs are relative to *http://localhost:2024*

Method | HTTP request | Description
------------- | ------------- | -------------
[**analyze_abi**](DefaultApi.md#analyze_abi) | **POST** /call/AnalyzeABI | 
[**analyze_contract_for_handlers**](DefaultApi.md#analyze_contract_for_handlers) | **POST** /call/AnalyzeContractForHandlers | 
[**analyze_event**](DefaultApi.md#analyze_event) | **POST** /call/AnalyzeEvent | 
[**analyze_layout**](DefaultApi.md#analyze_layout) | **POST** /call/AnalyzeLayout | 
[**extract_resume**](DefaultApi.md#extract_resume) | **POST** /call/ExtractResume | 
[**summarize_conversation**](DefaultApi.md#summarize_conversation) | **POST** /call/SummarizeConversation | 
[**summarize_title**](DefaultApi.md#summarize_title) | **POST** /call/SummarizeTitle | 



## analyze_abi

> models::AbiAnalysisResult analyze_abi(analyze_abi_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**analyze_abi_request** | [**AnalyzeAbiRequest**](AnalyzeAbiRequest.md) |  | [required] |

### Return type

[**models::AbiAnalysisResult**](ABIAnalysisResult.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## analyze_contract_for_handlers

> models::ContractAnalysis analyze_contract_for_handlers(analyze_contract_for_handlers_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**analyze_contract_for_handlers_request** | [**AnalyzeContractForHandlersRequest**](AnalyzeContractForHandlersRequest.md) |  | [required] |

### Return type

[**models::ContractAnalysis**](ContractAnalysis.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## analyze_event

> models::EventAnalyzeResult analyze_event(analyze_event_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**analyze_event_request** | [**AnalyzeEventRequest**](AnalyzeEventRequest.md) |  | [required] |

### Return type

[**models::EventAnalyzeResult**](EventAnalyzeResult.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## analyze_layout

> models::LayoutAnalysisResult analyze_layout(analyze_layout_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**analyze_layout_request** | [**AnalyzeLayoutRequest**](AnalyzeLayoutRequest.md) |  | [required] |

### Return type

[**models::LayoutAnalysisResult**](LayoutAnalysisResult.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## extract_resume

> models::Resume extract_resume(extract_resume_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**extract_resume_request** | [**ExtractResumeRequest**](ExtractResumeRequest.md) |  | [required] |

### Return type

[**models::Resume**](Resume.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## summarize_conversation

> models::ConversationSummary summarize_conversation(summarize_conversation_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**summarize_conversation_request** | [**SummarizeConversationRequest**](SummarizeConversationRequest.md) |  | [required] |

### Return type

[**models::ConversationSummary**](ConversationSummary.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## summarize_title

> models::SessionTitle summarize_title(summarize_title_request)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**summarize_title_request** | [**SummarizeTitleRequest**](SummarizeTitleRequest.md) |  | [required] |

### Return type

[**models::SessionTitle**](SessionTitle.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

